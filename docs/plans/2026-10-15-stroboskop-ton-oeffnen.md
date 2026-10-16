# Plan 2026-10-15: Stroboskop-Ansicht, „Ton aus", „Öffnen" im Player

Stand: 2026-10-15 · Stroboskop-Ansicht aus dem Nutzer-Wunsch (Max-Patches
`dr22`/`lissa-2020optic`: Bild wird mit der Lissajous-Phase durchs Bild
geschoben, der Scan liest die verschobene Ansicht). „Ton aus" und „Öffnen" sind
Nutzerwünsche von heute Abend.

## Entscheidungen

- **Stroboskop**: *eine* Bildquelle wird durchs Bild geschoben (rechts raus,
  links rein); dass es wie ein Vielfaches aussieht, ist nur der Effekt. Der Scan
  liest die Quellspalte `(Ansicht − Versatz) mod Breite`. Kein zweiter Puffer.
- **Ton aus**: schaltet nur den **lokalen** Klang der App ab (Tongenerator bzw.
  Modul-Samples); OSC zu Renoise bleibt unberührt.
- **Öffnen**: eigener Datei-Browser in der App (kein Java/SAF), weil die
  Activity `android.app.NativeActivity` ist und kein `onActivityResult` hat.
  Start: App-Modulordner, Download, Documents, App-Verzeichnis.

## Step 1: Kern – Stroboskop-Versatz

- Goal: Versatz und Quellspalten-Abbildung als getestete Kernfunktionen.
- Files: `core/src/stroboskop.rs` (neu), `core/src/lib.rs`.
- Test: `versatz_beginnt_bei_null`, `versatz_laeuft_in_eine_richtung`,
  `versatz_in_pixeln`, `quell_spalte_wickelt_um`.
- Verify: `cargo test --manifest-path core/Cargo.toml stroboskop` → erwartet: 4 grün.
- Status: **fertig** (250 Tests grün).

## Step 2: App – Stroboskop-Zeile, verschobene Ansicht, Scan auf der Ansicht

- Goal: Bild-Tab und B2 zeigen das Bild geschoben; der Scan liest die Ansicht.
- Files: `android/src/image.rs`.
- Test: Kern-Tests aus Step 1; Sichtprüfung am Emulator.
- Verify: Emulator mit bewegter Quelle: `Stroboskop` an, Richtung ▶, `÷2 ×2 ÷3 ×3`
  → Screenshots zeigen das durchlaufende Bild; Scan-Treffer sitzen auf dem
  verschobenen Inhalt; `logcat` ohne Fehler.
- Status: **umgesetzt, Beleg offen** (Emulator-Kamera liefert kein Bild;
  Ersatzquelle wird parallel recherchiert).

## Step 3: App – „Ton aus" in Bild und B2

- Goal: Ein Knopf in beiden Ansichten schaltet den lokalen Klang des Scans ab.
- Files: `android/src/image.rs` (Feld `ton_an`, früher Ausstieg in
  `note_lokal`, Knopf in `zeilen_kamera` – die B2-Leiste ruft dieselbe Zeile).
- Test: keine Kernlogik (Schalter + früher Ausstieg); Sichtprüfung + Log.
- Verify: Emulator: Knopf drücken → `logcat` zeigt `bild: Ton aus`; mit „Ton an"
  und Scan an kommt wieder ein Klang (klang-Thread-Meldung bzw. Note).
- Status: **umgesetzt, Beleg offen**.
- Depends on: —

## Step 4: Kern – Datei-Browser-Logik

- Goal: Sortieren/Filtern/Elternpfad als getestete Kernfunktionen, damit die
  App nur noch zeichnet.
- Files: `core/src/modulebrowser.rs` (neu), `core/src/lib.rs`.
- Test: `nur_moduldateien`, `ordner_zuerst_und_alphabetisch`, `elternpfad_kuerzt_den_pfad`.
- Verify: `cargo test --manifest-path core/Cargo.toml modulebrowser` → 3 grün.
- Depends on: —

## Step 5: App – „Öffnen" im Player

- Goal: Knopf „Öffnen …" öffnet ein Fenster mit Ordner-/Dateiliste; Tippen auf
  einen Ordner wechselt hinein, „↑" geht hoch, Tippen auf eine Moduldatei lädt
  und spielt sie.
- Files: `android/src/browser.rs` (neu), `android/src/lib.rs` (Knopf, Fenster,
  `modul_laden`), `android/src/paths.rs` (unverändert).
- Test: Kern-Tests aus Step 4; Sichtprüfung.
- Verify: Emulator: „Öffnen" → Fenster; ein per `adb push` abgelegtes Modul
  (`/sdcard/Android/data/org.jamcore.app/files/module/…it`) antippen →
  `logcat` „spielt: …"; „↑" und Ordnerwechsel sichtbar im Screenshot.
  (Download/Documents sind ohne Storage-Berechtigung gesperrt — die Knöpfe
  zeigen deshalb nur den App-Ordner.)
- Status: **umgesetzt, Beleg offen**.
- Depends on: Step 4

## Rollback

Stroboskop: Knopf/Zeile in `image.rs` entfernen (Flag `strobo_an` bleibt aus →
alles wie vorher). „Ton aus": Flag `ton_an` auf `true` festnageln. „Öffnen":
Knopf + `browser.rs` entfernen.

## Fixes aus dem Review (2026-10-16)

| Punkt | Fix |
|---|---|
| **Major 1**: „Ton aus" stoppte nur *neue* Noten — laufende Modul-Samples klangen weiter | Kanaäle werden gemerkt (`noten_kanaele`), `instr::spielen_mit_kanaelen`, neu `ModulQuelle::note_stoppen`, neu `klang::stimmen_aus()` (Zähler, der Klang-Thread leert seine Stimmenliste); „Ton aus" ruft beides |
| **Major 2**: `/sdcard/Download`/`Documents` sind ohne Storage-Berechtigung gesperrt | Startordner nur noch App-`module`/App-Ordner; Plan-Verify auf den App-Ordner umgestellt |
| Minor 3: B2 hatte die Stroboskop-Zeile nicht | `zeile_stroboskop` in der B2-Leiste (links) ergänzt |
| Minor 4: Tipp neben `bild_rect` (auf die Kopien) wurde ignoriert | `bild_tipp_erlaubt`: im Strobo-Fall zählt der ganze Kasten in x |
| Minor 5: Richtungs-/Ratenwechsel sprang | `strobo_phase_erhalten()` rechnet „Takte seit Start" auf denselben Versatz um; zusätzlich `÷3/×3 Takt` |
| Minor 6: Player-Knopfreihe lief rechts raus | `horizontal_wrapped` |
| Minor 7: Browser-Fenster lag unter den B2-Leisten | `.order(egui::Order::Foreground)` |
| Minor 8/9: Tests deckten Richtung bzw. negative Spalten nicht ab | Test prüft jetzt t=0,5 (rechts 0,25 / links 0,75) und `quell_spalte(-1,0,100)`/`(-250,0,100)` |
| Minor 10: Plan versprach 4 Browser-Tests | auf 3 korrigiert |
| Minor 11: doppelte Endungsliste (`paths.rs`) | `paths::scan` nutzt `jamcore::modulebrowser::ist_modul` |
