# JamCore Android – Bedienungsanleitung

Stand: 15. Oktober 2026 · Beschreibt die App im Quellbaum (`android/`).
Funktionen mit **„in Arbeit"** oder **„geplant"** sind noch nicht fertig.

## Inhalt

- [Was ist JamCore?](#was-ist-jamcore)
- [Voraussetzungen](#voraussetzungen)
- [Installation aus dem Quellbaum](#installation-aus-dem-quellbaum)
- [Die Tabs](#die-tabs)
- [Typische Abläufe](#typische-abläufe)
- [USB-Nummernblock](#usb-nummernblock)
- [Wo liegen meine Daten?](#wo-liegen-meine-daten)
- [Problemlösung](#problemlösung)
- [Geplante Funktionen](#geplante-funktionen)

## Was ist JamCore?

JamCore ist eine Android-App zum Musizieren mit Modulen und Bildern. Alles läuft
auf dem Handy; der Laptop ist nur für Renoise und Ableton Link nötig.

- **Modul-Player** – spielt `.it/.xm/.mod/.s3m/.mtm`-Dateien.
- **Rhythmus-Raster** – Step-Editor mit 8 Patterns, sendet Noten per OSC an Renoise.
- **Bild-Scan** – Kamera oder Videos live scannen; Farben werden zu Noten.
- **Takt-Maske** – Zeitabstände tippen oder wischen; daraus wird die grüne Leiste im Bild.
- **OSC** – Noten, Transport und Tempo an Renoise (oder ein Relay auf dem Laptop).
- **Ableton Link** – Tempo und Takt mit anderen Geräten teilen.
- **Tagebuch** – Ideen mit Name/Notiz sichern, Bild-Pattern als Renoise-XML kopieren.

## Voraussetzungen

- Android-Gerät mit **arm64**-Prozessor (arm64-v8a), mindestens Android 8 (minSdk 26).
  Getestet: Pixel 4a (Android 13) und Pixel 10 Pro (Android 17).
- Beim ersten Start den Zugriff auf **Kamera** und **Mikrofon** erlauben.
  Sonst startet die Kamera nicht; im Protokoll steht dann
  `Permission Denial ... camera permission`.
- Für OSC und Link: Handy und Laptop im **selben Netz** (WLAN oder Handy-Hotspot)
  und auf dem Laptop Renoise bzw. das OSC-Relay.

## Installation aus dem Quellbaum

1. Terminal im Projektordner öffnen und `nix-shell` starten – darin liegen
   Android-SDK/NDK, gradle und jdk17.
2. Handy per USB anschließen (USB-Debugging in den Entwickleroptionen einschalten)
   und `bash android/build-apk.sh --install` ausführen.
   - Das Skript baut die Rust-Bibliothek per `cargo ndk` (arm64-v8a, Platform 26),
     packt die APK mit Gradle und installiert sie per `adb install -r`.
     Danach startet es die App.
   - Ergebnis: `android/app/build/outputs/apk/debug/app-debug.apk`. Das ist eine
     **Debug-APK** (Debugsignatur, ohne Optimierungen) – zum Ausprobieren, nicht
     zur Weitergabe.
3. Beim ersten Start die Berechtigungsdialoge für Kamera und Mikrofon erlauben.
4. Mitlesen, was die App tut: `adb logcat -s jamandroid`.

## Die Tabs

Die Tab-Leiste liegt oben unter dem Titel „JamCore". Die Reihenfolge:

| Tab | Was man dort macht | Wichtigste Regler |
|---|---|---|
| **Überblick** | Kamera starten/stoppen, Module finden, Ordner-Zugriff erlauben | „Kamera an"/„Kamera aus", „Module neu suchen", „Ordner-Zugriff erlauben" (mit Anzeige „Alle Dateien: erlaubt/nein"), „Zum Player" |
| **Player** | Module auswählen und abspielen | „Neu suchen", „**Öffnen …**" (Datei-Browser), „Abspielen", „Stopp", „Dauerschleife", „WAV sichern"; Anzeige Peak/Position/Order/Row; darunter die **Akkord-Anzeige** der gerade angespielten Noten |
| **OSC** | Ziel-Laptop einstellen, Renoise ansteuern, Tempo senden | IP, Port (z. B. 7005), „Verbinden"/„Trennen", Tempo 30–300, „Tempo senden", Notenknöpfe 48/60/64/67/72, „Start"/„Stop", „BPM 128", „Follow an/aus" |
| **Rhythmus** | Schritt-Raster bearbeiten (8 Patterns) | Zellen tippen, „Laenge" 1–1024, „Rendern", „Follow", „Klang" (Modul-Samples/Tongenerator), „Vel -/+", Skala, „CSV/XML sichern" |
| **Tempo** | Tempo per Mikrofon messen | „Messen (4 s)", „Live", „Stop", „Tap", „Übernehmen"; Anzeige Pegel/Konfidenz |
| **Bild** | Kamera/Video live scannen, Farbe wählen, Maske setzen | „Kamera an/aus", „Scan an/aus", „**Ton aus**" (nur der App-Klang; OSC bleibt), „**Stroboskop**" (Bild läuft im Takt durchs Bild, der Scan liest die bewegte Ansicht), Note-Modus, „ins Pattern", „Maske", Toleranz, Zoom/Bereich, „Schritte", „Datei sichern/laden" |
| **Kreis** | Quintenzirkel als Klangrad | Segmente antippen (sendet sofort `note_on`), „Auswahl löschen" |
| **Session** | Ableton Link anzeigen und teilen (**wird mit Tempo zu einem Tab zusammengelegt**) | „Link an/aus", „Link neu starten", „Tempo folgen", Quantum 1–16 |
| **Tagebuch** | Ideen sichern (Notiz + aktuelles Raster/Tempo) | „Eintrag speichern", „Neu laden", Einträge-Liste, oben der Bild-Pattern-Export (Renoise-XML kopieren) |
| **Maske** | Takt-Maske: Zeitabstände tippen/wischen | Tastenfeld, „je Wert"/„verteilt", „Spalten", „Wert max", „Wisch-Modulation", „neu", „leeren", min/max-Regeln, Zoom |
| **Kombi** | Zwei Ansichten gleichzeitig (Hochformat untereinander, Querformat nebeneinander) | Auswahl oben/unten bzw. links/rechts (leer/Bild/Rhythmus/Maske/Tempo/Session/Tagebuch/Kreis) |
| **Maske 2 (M2)** | 10er-Tastenfeld in Nummernblock-Anordnung, mit „Ende" | Tastenfeld, „neu"/„Ende"; Nummernblock wirkt auch hier |
| **Bild 2 (B2)** | Bild im Vollbild mit transparenten Leisten | gleiche Regler wie „Bild" (Kamera/Scan/Maske), Leisten scrollen |
| **Instrumente (Instr.)** | Instrumente/Samples mit Notenbereichen (von/bis) | Eintragsliste, „von"/„bis"-Regler, „×" zum Löschen eines Bereichs |
| **Lissa** | Lissajous-Figur, die im Takt kippt (Phasendrehung) | X/Y-Stepper (1–16) + Presets, „Umdrehungen" (1/64 … 16) mit Stufe „∞", „Takte pro Zyklus", „Sägezahn (Original)"/„Weich" bzw. „◀ links"/„rechts ▶", „Ton" + Grundton, „Präsentation" |

Wichtig zu wissen:

- Es gibt **ein Tempo** für Takt-Scan, Rendern und Tagebuch. Quelle ist der
  OSC-Tab (30–300 BPM); mit aktivem „Tempo folgen" gewinnt das Link-Tempo.
- Die **Patternlänge** („Laenge" im Rhythmus-Tab, „Schritte" im Bild-Tab,
  Vorgabe 512) ist gleichzeitig die X-Auflösung des Bild-Scans und das Raster
  der Maske. Beide Tabs schreiben denselben Wert.
- Die Klangquelle (Modul-Samples oder Tongenerator) stellst du **einmal** im
  Rhythmus-Tab ein; der Bild-Scan klingt mit derselben Quelle.
- Die Noten der Module sind urheberrechtlich geschützt – vor Aufnahme, Export
  oder Weitergabe die Rechte klären (steht auch in der App).

## Typische Abläufe

### a) Takt-Maske aufnehmen und als grünen Balken im Bild nutzen

1. **Maske**-Tab öffnen. Jeder Tipp ins Tastenfeld setzt einen Zeitpunkt;
   ein Wisch über mehrere Felder zählt Anfang und Ende. Mit angestecktem
   Nummernblock treffen die Ziffern 1…9, 0 die ersten zehn Positionen.
2. „neu" leert die Aufnahme und beginnt von vorn. In der alten Maske läuft die
   Aufnahme weiter, bis du „neu" drückst. In **Maske 2** hält „Ende" (oder
   Enter am Nummernblock) die Folge fest; erst „neu" nimmt wieder auf.
3. Unter dem Feld liegt die Zeitleiste. Tippe einen Abschnitt an und stelle mit
   „min"/„max" die Schrittwerte nur für diesen Abschnitt ein; „Regel weg" nimmt
   die Regel zurück.
4. Mit „Wisch-Modulation" bestimmst du, wie stark die Wischgeschwindigkeit die
   Schrittweite krümmt (0 = aus).
5. **Bild**-Tab öffnen und „Kamera an" drücken. Die Zeilen mit „Kamera an" und
   „Scan an" bleiben immer sichtbar.
6. Auf eine Farbe im Bild tippen (gedrückt halten friert das Bild ein und wählt
   die Farbe unter dem Finger). Mit „Toleranz" und „R/G/B" feinjustieren.
7. Der **grüne Balken** oben im Bild zeigt die Maske: grün = Spalte kommt durch,
   rot = Spalte ist gesperrt. Der Schalter „Maske" schaltet sie für den Scan an
   oder aus.
8. „Scan an" startet den Spalten-Sweep. Achtung: „ins Pattern" einschalten,
   wenn die Treffer zusätzlich im Rhythmus-Raster landen sollen.

### b) Renoise über OSC ansteuern

1. Beide Geräte ins selbe Netz bringen (WLAN oder Handy-Hotspot).
2. **OSC**-Tab: IP = IP des Laptops, Port = der Port, auf dem Renoise oder das
   Relay lauscht (z. B. 7005), dann „Verbinden". Die Statuszeile zeigt
   „verbunden mit …".
3. Auf dem Laptop die Firewall öffnen: UDP **7000–7010** (nötig für Renoise und
   die Werkzeuge).
4. Noten und Transport testen: Knöpfe 48/60/64/67/72, „Start"/„Stop",
   „BPM 128", „Follow an/aus".
5. Scan-Noten: Im **Bild**-Tab „Scan an" – die Treffer gehen als
   `/renoise/trigger/note_on` an den eingestellten Port (dazu ein MIDI-Trigger
   auf Port+2).
6. Tempo: Im OSC-Tab den Regler 30–300 stellen und „Tempo senden" drücken; mit
   „Teiler" geht zusätzlich ein gefaltetes Tempo an Port+1 (Port 7001).
   „Übernehmen" im **Tempo**-Tab setzt das App-Tempo und sendet es ebenfalls.
7. **adb reverse kann kein UDP.** OSC funktioniert nur, wenn Handy und Laptop
   wirklich im selben Netz sind und die Laptop-IP im OSC-Tab steht.

### c) Rhythmus: Noten setzen, Patternlänge, Rendern, Klangquelle

1. **Rhythmus**-Tab öffnen und oben ein Pattern wählen (1–8).
2. Unter „Laenge" die Patternlänge einstellen: 1–1024 Schritte, Vorgabe 512.
   Der Knopf „16" setzt 16. Dieselbe Zahl gilt im Bild-Tab.
3. Zellen im Raster antippen: Die Zeilen sind 36/48/60/72, die Blöcke je 8
   Schritte. Ein Tipp setzt die Note, ein zweiter Tipp auf dieselbe Zelle
   löscht sie. Die Note klingt sofort und geht per OSC raus.
4. „Vel -/+" stellt die Lautstärke neue Noten ein.
5. „Klang" wählen: **Modul-Samples** (nur wenn ein Modul im Player geladen ist)
   oder **Tongenerator** (Knopf mit dem Wellennamen wechselt die Welle).
   Ist „Modul-Samples" gewählt und kein Modul geladen, klingt der Tongenerator.
6. „Rendern" spielt das Raster als 16tel-Schleife mit dem App-Tempo ab
   (Anzeige „… BPM (folgt App-Tempo)"). „Rendern stoppen" hält an. Mit
   „Transport" startet/stoppt dazu der Renoise-Transport.
7. „Slot spielen" spielt das Pattern einmal, auf die eigene 16tel quantisiert.
8. „CSV sichern"/„XML schreiben" legen die Dateien in `patterns/` ab;
   „Schism kopieren" legt ein Pattern in die Zwischenablage.
9. **In Arbeit:** Der Tongenerator soll polyphon werden (Stimmen und Tonlänge
   einstellbar). Heute klingt er einstimmig.

## USB-Nummernblock

- Nummernblock per OTG-Adapter anschließen; die App muss im Vordergrund sein.
- **Maske:** Die Ziffern 1…9 und 0 treffen die ersten zehn Positionen in
  Leserichtung (1 = erste Position, 0 = zehnte; Positionen, die das Feld nicht
  hat, werden ignoriert).
- **Maske 2:** Die Ziffer trifft den **räumlich gleichen** Knopf
  (7-8-9 / 4-5-6 / 1-2-3 / 0). Enter wirkt als „Ende".
- **In Arbeit:** Die Nummernblock-Anbindung ist neu und im aktuellen Baum noch
  nicht in die App eingebunden (Stand 14.10.2026).

## Wo liegen meine Daten?

Basisordner auf dem Handy:

```
/storage/emulated/0/Android/data/org.jamcore.app/files/
```

| Ort | Inhalt |
|---|---|
| `bild/` | Standbilder für den Bild-Tab (wie `bilder/`) |
| `audio/` | Audiodateien für den Slicer (`.wav/.mp3/.ogg/.flac/.m4a/…`) |
| `module/` | Module für den Player (`.it/.xm/.mod/.s3m/.mtm`) |
| `videos/` | Videoquellen für den Bild-Tab (mp4/mkv/webm/avi/mov/3gp) |
| `recordings/` | Mikrofon-Aufnahmen des Slicer-Tabs (`aufnahme-N.wav`) |
| `bilder/` | Standbilder für den Bild-Tab (png/jpg/jpeg/bmp) |
| `patterns/` | `pattern N.csv` und `pattern N.xml` aus dem Rhythmus-Tab |
| `tagebuch/` | Tagebuch-Einträge (`JJJJ-MM-TT_hh_mm.txt`) |
| `wav/` | WAV-Render aus dem Player-Tab („WAV sichern") |
| `osc.txt` | gemerktes OSC-Ziel (Zeile 1 Host, Zeile 2 Port) |
| `KeyEventRender.txt` | Takt-Maske zum Austausch; wird im Bild-Tab über „Datei sichern"/„Datei laden" geschrieben/gelesen |
| `frame4save.bmp` | letztes Bild aus „Snapshot" im Bild-Tab |

Die Ordner `bild`, `audio`, `module`, `videos` und `recordings` legt die App
beim Start selbst an.

Am einfachsten kommst du an die Dateien per `adb pull` oder `adb shell`
(Dateimanager dürfen auf `Android/data` oft nicht zugreifen).

### Wo liegen meine Dateien?

Zwei Orte, je nachdem, wie viel Zugriff du der App geben willst:

- **App-Ordner** (Tabelle oben): brauchen **keine** Berechtigung und sind vom
  Rechner per USB erreichbar. Leg dort deine Bilder, Audiodateien und Module
  ab; sie erscheinen im Player („Öffnen …") und im Slicer („Audio öffnen").
- **Systemordner** `DCIM`, `Pictures`, `Music`, `Download` und `Documents`:
  nur mit der Berechtigung **„Alle Dateien"** (Android 11+). Im
  **Überblick**-Tab steht der Stand als „Alle Dateien: erlaubt/nein";
  „Ordner-Zugriff erlauben" öffnet die Systemseite für die App. Dort
  „Zugriff auf alle Dateien erlauben" einschalten und zurück zur App — die
  Anzeige springt nach etwa einer Sekunde auf „erlaubt". Danach stehen die
  Systemordner als Startordner in den Browsern bereit (im Player zusätzlich
  Download und Documents, im Slicer zusätzlich Music, Download und
  Documents).
- **Ohne die Berechtigung** passiert nichts Schlimmes: die Systemordner fehlen
  einfach in den Browsern. Nicht lesbare Ordner werden als Meldung im
  Browserfenster angezeigt („… nicht lesbar (Berechtigung?)") und nur ins
  Protokoll geschrieben — die App stürzt nicht ab.

## Problemlösung

| Problem | Ursache | Lösung |
|---|---|---|
| Kamera startet nicht | Berechtigung fehlt oder wurde abgelehnt | In den App-Einstellungen Kamera erlauben, App neu starten, „Kamera an" erneut drücken. Im Log steht bei fehlender Berechtigung `Permission Denial ... camera permission`. |
| OSC kommt nicht an | Falsche IP/Port, anderes Netz oder Firewall blockt UDP | Laptop-IP im OSC-Tab prüfen, „Verbinden" drücken, beide Geräte ins selbe Netz, Firewall UDP 7000–7010 öffnen. `adb reverse` kann **kein** UDP. |
| Handy sperrt im Test | Bildschirm-Timeout | `adb shell svc power stayon usb` (bleibt wach, solange USB steckt) und in den Einstellungen → Display den Timeout erhöhen. |
| Bildquelle schwarz | Keine Kamera-Vorschau („Keine Vorschau (Kamera an?)"), noch keine Quelle gestartet oder Schwarz ist die eingestellte Hintergrundfarbe und wird übersprungen | Kamera starten bzw. „Quelle an" drücken; im Bild eine andere Farbe antippen; Statuszeile auf Fehlermeldungen prüfen. |
| Tastatur erreicht Felder nicht | Textfeld ist nicht aktiv | Das Feld (z. B. IP oder Port im OSC-Tab) direkt antippen und dann tippen. Der USB-Nummernblock braucht kein Textfeld – er wirkt in den Masken. |

## Geplante Funktionen

Aus dem Plan vom 14.10.2026 – **noch nicht fertig**:

- **Maske 2 (M2):** 10er-Feld in Nummernblock-Anordnung, „Ende" beendet die
  Aufnahme bis „neu". **In Arbeit.**
- **USB-Nummernblock:** Ziffern treffen in beiden Masken die Tasten, Enter ist
  „Ende" in Maske 2. **In Arbeit.**
- **Ein BPM:** Tempo und Session werden **ein** Tab (Messung, Link, BPM, Tap);
  Regler in Rhythmus und Bild schreiben denselben Wert. **Geplant.**
- **Bild 2 (B2):** Bild im Vollbild mit transparenten, scrollbaren Leisten;
  derselbe Zustand wie „Bild". **In Arbeit.**
- **Polyphoner Tongenerator:** 1–256 Stimmen (Vorgabe 8), Tonlänge 1/64 bis
  4 Takte. **Geplant.**
- **Rhythmus-Bündelung:** mehrere Schritte je Zelle (1–32) zusammenfassen.
  **Geplant.**
- **Instrumente (Instr.):** Instrumente und Samples mit Notenbereichen –
  überlappende Bereiche klingen zusammen, außerhalb bleibt es still.
  **In Arbeit.**
