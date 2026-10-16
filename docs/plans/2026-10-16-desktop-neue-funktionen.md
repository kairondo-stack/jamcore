# Plan 2026-10-16: Desktop-Paket – Stroboskop, Lissa, Ton aus, Öffnen, Handy-Relay

Stand: 2026-10-16 · Nutzerwünsche: die neuen App-Funktionen auch im
Desktop-Paket (`~/jam-nix`), plus ein Relay, das **alle Handys mit JamCore**
(bis 10) an Renoise weitergibt — wahlweise auf ein gemeinsames Ziel oder
**je Handy ein Instrument**.

## Entscheidungen

- **Stroboskop in rrn5**: wie in der App — eine Bildquelle läuft im Takt durchs
  Bild (Lissajous-Phase, endlos, Richtung), der Scan liest die verschobene
  Quellspalte (`jamcore::stroboskop`).
- **Lissa als eigenes Desktop-Programm**: neuer Bin `lissa` (eigene Crate wie
  rrn5/imgsh), SDL2-Fenster + SDL2-Audio, Mathematik aus `jamcore::lissa`.
- **Ton aus / Öffnen** in rrn5 (und pattern_recorder für „Ton aus").
- **Relay in bundle_udp**: bis zu **10 Handys** (Quell-IPs) werden gezählt und
  alles an Renoise weitergegeben; zusätzlich ist **je Handy ein eigenes Ziel**
  (Instrument/Port) konfigurierbar. Logik im Kern (`jamcore::relay`), testbar
  ohne Netz.

## Step 1: Kern – Relay-Logik (TDD)

- Goal: Quellenverwaltung und Zielwahl als reine, getestete Funktionen:
  Quellen registrieren (max 10, älteste fällt raus), Quelle → Ziel abbilden
  (gemeinsames Ziel oder je Quelle ein eigenes), Anzeige „N/10 Handys".
- Files: `core/src/relay.rs` (neu), `core/src/lib.rs`.
- Test: `max_zehn_quellen`, `aelteste_faellt_raus`, `gemeinsames_ziel`,
  `je_quelle_ein_ziel`, `ziel_fuer_unbekannte_quelle`, `kurztext`.
- Verify: `cargo test --manifest-path core/Cargo.toml relay` → grün.
- Depends on: —

## Step 2: bundle_udp – mehrere Handys → Renoise

- Goal: Der Relay lauscht wie bisher auf 7000/7005, zählt bis zu 10 Quell-IPs,
  gibt alles an Renoise weiter und zeigt den Zustand; je Quelle optional ein
  eigenes Ziel (Umgebungsvariable/Argument, z. B. `OSC_MAP=10.179.32.50:7005,…`).
- Files: `bundle_udp/src/main.rs` (nutzt `jamcore::relay`), ggf.
  `bundle_udp/Cargo.toml` (jamcore-Abhängigkeit).
- Test: Kern-Tests aus Step 1; Sichtprüfung: zwei lokale UDP-Sender → beide
  Pakete kommen am Ziel an, Log zeigt „2/10 Handys".
- Verify: `cargo run --release --bin bundle_udp` + `nc -u`-Proben.
- Depends on: Step 1

## Step 3: Lissa als Desktop-Programm

- Goal: Fenster mit der Lissajous-Figur (X/Y-Stepper, Presets, Umdrehungen,
  Takte pro Zyklus, Sägezahn/weich/∞ links-rechts) und **zwei Tongenerator-
  Stimmen** (Grundton + X/Y-Verhältnis, X mit dem Frequenzversatz der Drehung).
- Files: `lissa/Cargo.toml` (neu, SDL2 + jamcore), `lissa/src/main.rs` (neu).
- Test: Mathematik kommt aus `jamcore::lissa` (getestet); Sichtprüfung.
- Verify: `cargo run --release --bin lissa` → Fenster zeigt die kippende Figur,
  Ton hörbar, Tasten/SDL-Dialog wie bei den anderen Tools.
- Depends on: —

## Step 4: rrn5 – Stroboskop, Ton aus, Öffnen

- Goal: rrn5 bekommt (a) die Stroboskop-Ansicht samt Scan auf der Ansicht,
  (b) „Ton aus" (lokaler Klang; OSC bleibt), (c) einen Datei-Browser zum
  Auswählen des Bildordners/-bilds.
- Files: `rrn5/src/main.rs`, `rrn5/src/strobo.rs` (neu, dünn über
  `jamcore::stroboskop`), `rrn5/Cargo.toml` (jamcore).
- Test: `jamcore::stroboskop`-Tests; Sichtprüfung mit zwei Bildern.
- Verify: rrn5 starten, Stroboskop an → Bild läuft, Treffer wandern mit;
  „Ton aus" stoppt den lokalen Klang; Datei-Browser lädt ein Bild.
- Depends on: —

## Step 5: Paket aktualisieren und bauen

- Goal: Quellen ins Desktop-Paket spiegeln und dort bauen.
- Files: `~/jam-nix` (Kopie; `./sync.sh`, `./update-hashes.sh`, `default.nix`
  um den neuen Bin `lissa` ergänzen).
- Test: `nix-build` bzw. `nix-shell --run "cargo build --release"` in `~/jam-nix`.
- Verify: `~/jam-nix/result/bin/…` listet rrn5, pattern_recorder, imgsh,
  bundle_udp, it_live **und lissa**; Start aus `Start.txt`.
- Depends on: Step 1–4

## Rollback

Alle Schritte sind additiv: neuer Kern-Modul(e), neuer Bin, Relay-Erweiterung.
Rücknahme je Datei möglich; `~/jam-nix` ist eine Kopie (Projekt bleibt Quelle).
