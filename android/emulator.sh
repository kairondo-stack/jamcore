#!/usr/bin/env bash
# Steuert den Android-Emulator fuer JamCore (statt Handy per USB/WLAN).
#
# Aufruf in der Nix-Shell (dort liegen emulator/avdmanager/adb bereit):
#   nix-shell --run 'bash android/emulator.sh <befehl>'
#
# Befehle:
#   create             AVD anlegen (idempotent; ueberschreibt nicht)
#   start [opts]       Emulator starten und auf boot_completed warten
#                      --headless   ohne Fenster (fuer Automatisierung)
#                      --wipe       Datenpartition loeschen (Factory-Reset)
#                      --cold       ohne Snapshot booten
#   app                JamCore starten (force-stop + am start)
#   restart            Emulator neu starten (stop + start)
#   stop               Emulator beenden (Snapshot wird gespeichert)
#   status             Laufende Geraete und Boot-Status
#   logcat [args...]   Standard: nur jamandroid-Meldungen; sonst adb logcat-Args
#   shot [datei]       Screenshot (PNG) des Emulators
#   tap X Y            Tap in Geraete-Koordinaten
#   install            x86_64-APK bauen, installieren, Activity starten
#   wipe               Naechster Start mit leerer Datenpartition
#   shell              adb shell im Emulator
#
# Umgebung: AVD_NAME, SYSTEM_IMAGE (Default API 36 AOSP x86_64), DEVICE_PROFILE,
#           GPU (Default host = echte GPU; swiftshader_indirect heizt die CPU:
#           2026-10-21 gemessen 480–680 % statt 12 % und 82–89 °C statt 60 °C),
#           PORT (Default 5554).
set -euo pipefail

ROOT="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
AVD_NAME="${AVD_NAME:-jamcore_api36}"
SYSTEM_IMAGE="${SYSTEM_IMAGE:-system-images;android-36;default;x86_64}"
DEVICE_PROFILE="${DEVICE_PROFILE:-pixel_6}"
GPU="${GPU:-host}"
PORT="${PORT:-5554}"
SERIAL="emulator-$PORT"
LOG="${ANDROID_EMULATOR_HOME:-$ROOT/.android}/emulator.log"
WIPE_FLAG="${ANDROID_EMULATOR_HOME:-$ROOT/.android}/.wipe-requested"

need_shell() {
  if ! command -v emulator >/dev/null || ! command -v avdmanager >/dev/null; then
    echo "FEHLER: emulator/avdmanager nicht im PATH." >&2
    echo "        Bitte in der Nix-Shell ausfuehren: nix-shell" >&2
    exit 1
  fi
}

avd_exists() {
  avdmanager list avd 2>/dev/null | grep -q "Name: $AVD_NAME$"
}

cmd_create() {
  need_shell
  if avd_exists; then
    echo "AVD '$AVD_NAME' existiert schon ($ANDROID_AVD_HOME/$AVD_NAME.avd)"
    return 0
  fi

  echo no | avdmanager create avd --force \
    --name "$AVD_NAME" \
    --package "$SYSTEM_IMAGE" \
    --device "$DEVICE_PROFILE"

  # Hardwareprofil anpassen: 4 Kerne, 4 GB RAM, 6 GB Datenpartition,
  # Host-Tastatur (praktisch fuer adb/Tippen), Audio rein/raus, Kamera
  # als virtuelle Szene. Die Zeilen stehen in config.ini; avdmanager hat
  # dafuer keine Optionen.
  local cfg="$ANDROID_AVD_HOME/$AVD_NAME.avd/config.ini"
  set_cfg() {
    local key="$1" value="$2"
    if grep -q "^$key=" "$cfg"; then
      sed -i "s|^$key=.*|$key=$value|" "$cfg"
    else
      printf '%s=%s\n' "$key" "$value" >> "$cfg"
    fi
  }
  set_cfg hw.cpu.ncore 4
  set_cfg hw.ramSize 4096
  set_cfg disk.dataPartition.size 6442450944
  set_cfg hw.keyboard yes
  set_cfg hw.audioInput yes
  set_cfg hw.audioOutput yes
  set_cfg hw.gpu.enabled yes
  set_cfg hw.gpu.mode "$GPU"
  set_cfg hw.camera.back virtualscene
  set_cfg hw.camera.front emulated
  set_cfg showDeviceFrame no
  set_cfg fastboot.forceColdBoot no

  echo "AVD '$AVD_NAME' angelegt: $ANDROID_AVD_HOME/$AVD_NAME.avd"
}

cmd_start() {
  need_shell
  local headless=0 cold=0 wipe=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --headless) headless=1 ;;
      --wipe) wipe=1 ;;
      --cold) cold=1 ;;
      *) echo "Unbekannte Option: $1" >&2; exit 2 ;;
    esac
    shift
  done

  avd_exists || cmd_create

  if adb -s "$SERIAL" get-state >/dev/null 2>&1; then
    echo "Emulator laeuft schon ($SERIAL)"
    return 0
  fi

  local args=( -avd "$AVD_NAME" -port "$PORT" -gpu "$GPU" -no-boot-anim
               -netdelay none -netspeed full -no-metrics )
  [ "$headless" = 1 ] && args+=( -no-window -no-audio )
  [ "$cold" = 1 ] && args+=( -no-snapshot-load )
  if [ "$wipe" = 1 ]; then
    args+=( -wipe-data )
  elif [ -f "$WIPE_FLAG" ]; then
    args+=( -wipe-data )
    rm -f "$WIPE_FLAG"
  fi

  mkdir -p "$(dirname "$LOG")"
  echo "Starte Emulator '$AVD_NAME' (Log: $LOG)"
  nohup emulator "${args[@]}" >"$LOG" 2>&1 &

  echo -n "Warte auf Boot"
  local waited=0
  until [ "$(adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; do
    sleep 2
    waited=$((waited + 2))
    if [ "$waited" -ge 300 ]; then
      echo
      echo "FEHLER: Emulator nach 300 s nicht gebootet. Letzte Logzeilen:" >&2
      tail -20 "$LOG" >&2
      exit 1
    fi
    if ! kill -0 "$(pgrep -f "avd $AVD_NAME" | head -1)" 2>/dev/null; then
      echo
      echo "FEHLER: Emulator-Prozess beendet. Log:" >&2
      tail -20 "$LOG" >&2
      exit 1
    fi
    echo -n "."
  done
  echo
  adb -s "$SERIAL" shell wm dismiss-keyguard >/dev/null 2>&1 || true

  # Nach einem Quick-Boot-Snapshot bleibt der GL-Kontext der schon laufenden App
  # schwarz (Snapshot-Restore). Deshalb einmal beenden; 'app' startet sie frisch.
  if adb -s "$SERIAL" shell pm list packages 2>/dev/null | grep -q "package:org.jamcore.app"; then
    if adb -s "$SERIAL" shell pidof org.jamcore.app >/dev/null 2>&1; then
      adb -s "$SERIAL" shell am force-stop org.jamcore.app
      echo "(JamCore lief im Snapshot und wurde beendet -> 'bash android/emulator.sh app')"
    fi
  fi

  echo "Emulator bereit: $SERIAL ($SYSTEM_IMAGE)"
  adb devices
}

cmd_stop() {
  need_shell
  if ! adb -s "$SERIAL" get-state >/dev/null 2>&1; then
    echo "Kein Emulator auf $SERIAL."
    return 0
  fi
  adb -s "$SERIAL" emu kill >/dev/null 2>&1 || true
  for _ in $(seq 1 30); do
    adb -s "$SERIAL" get-state >/dev/null 2>&1 || { echo "Emulator gestoppt."; return 0; }
    sleep 1
  done
  echo "Emulator reagiert nicht mehr auf 'emu kill'; beende Prozess."
  pkill -f "avd $AVD_NAME" || true
}

cmd_status() {
  need_shell
  echo "AVD:            $AVD_NAME ($SYSTEM_IMAGE)"
  avd_exists && echo "AVD angelegt:   ja" || echo "AVD angelegt:   nein"
  adb devices
  if adb -s "$SERIAL" get-state >/dev/null 2>&1; then
    echo "boot_completed: $(adb -s "$SERIAL" shell getprop sys.boot_completed | tr -d '\r')"
    echo "AVD laut Geraet: $(adb -s "$SERIAL" shell getprop ro.boot.qemu.avd_name 2>/dev/null | tr -d '\r')"
    echo "Aufloesung:     $(adb -s "$SERIAL" shell wm size | tr -d '\r')"
  else
    echo "boot_completed: -"
  fi
}

cmd_logcat() {
  need_shell
  if [ $# -eq 0 ]; then
    set -- -v time -s jamandroid
  fi
  adb -s "$SERIAL" logcat "$@"
}

cmd_shot() {
  need_shell
  local out="${1:-/tmp/opencode/emulator-shot.png}"
  mkdir -p "$(dirname "$out")"
  adb -s "$SERIAL" exec-out screencap -p >"$out"
  echo "Screenshot: $out"
}

cmd_tap() {
  need_shell
  [ $# -eq 2 ] || { echo "Aufruf: tap X Y" >&2; exit 2; }
  adb -s "$SERIAL" shell input tap "$1" "$2"
}

cmd_install() {
  need_shell
  ABIS="x86_64" bash "$ROOT/android/build-apk.sh"
  local apk="$ROOT/android/app/build/outputs/apk/debug/app-debug.apk"
  adb -s "$SERIAL" install -r "$apk"
  adb -s "$SERIAL" shell am start -n org.jamcore.app/android.app.NativeActivity
  echo "gestartet; Log: bash android/emulator.sh logcat"
}

cmd_app() {
  need_shell
  adb -s "$SERIAL" shell am force-stop org.jamcore.app >/dev/null 2>&1 || true
  adb -s "$SERIAL" shell am start -n org.jamcore.app/android.app.NativeActivity
}

cmd_restart() {
  cmd_stop
  cmd_start "$@"
}

cmd_wipe() {
  mkdir -p "$(dirname "$WIPE_FLAG")"
  touch "$WIPE_FLAG"
  echo "Naechster Start loescht die Datenpartition ($WIPE_FLAG)."
}

case "${1:-}" in
  create)  shift; cmd_create "$@" ;;
  start)   shift; cmd_start "$@" ;;
  stop)    shift; cmd_stop "$@" ;;
  status)  shift; cmd_status "$@" ;;
  logcat)  shift; cmd_logcat "$@" ;;
  shot)    shift; cmd_shot "$@" ;;
  tap)     shift; cmd_tap "$@" ;;
  install) shift; cmd_install "$@" ;;
  app)     shift; cmd_app "$@" ;;
  restart) shift; cmd_restart "$@" ;;
  wipe)    shift; cmd_wipe "$@" ;;
  shell)   shift; need_shell; adb -s "$SERIAL" shell "$@" ;;
  *)       sed -n '2,30p' "$0"; exit 2 ;;
esac
