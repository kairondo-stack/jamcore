# Max-Patches vom USB-Stick — Übersicht und Auswahlvorschläge

Stand: 2026-10-15 · Quelle: USB-Stick `Untitled 2`, Ordner `max/` und `max2/`
(189 Patch-Dateien, davon **84 mit verschiedenem Inhalt** — der Rest sind Kopien
in Unterordnern). Methode: Patcher-JSON aus den `.amxd`-Containern entpackt
(`/tmp/opencode/max/steckbrief.pl`, `chain.pl`), Binär-Patcher per `strings`.
Fünf Subagenten haben je eine Gruppe analysiert; hier zusammengeführt.

Legende Kandidat: **App** = JamCore (Android), **Desk** = rrn5 / pattern_recorder,
**—** = nichts (Altlast/Duplikat).

## Gruppe 1: BPM / Tempo / Stroboskop („bpmoptisch"-Familie)

| Patch | Zweck | Kern | Kandidat |
|---|---|---|---|
| `bpmoptisch-feb2019.amxd` | größte Fassung: Tempo (Transport **und** MIDI-Clock über `rtin`→`sync~`) → tan/atan-Kette → Alias-Paar **2000±x** → `s bpmo`; MIDI-Clock raus (`midiout`), „wechslerspeeder" (metro 2n + 32n…0.125n) | `tan(f1·f3·π/f2)` → `·f2/f3` → `(atan(...)+f3·π)·f2/π`; Konstantentripel 2000/1970/30 und 1880/120 | App (Stroboskop-Idee, MIDI-Clock als Link-Ergänzung) |
| `bpmoptisch-feb2019-2.amxd` | reduzierte Fassung ohne MIDI: Handwerte → Kette → `bpmo` | dieselbe Kette, 2000/1969/31 und 2000/1876/124; Testpaar 87/86.132812/104 → „86.55" | — (Rechenkern-Referenz) |
| `bpmoptisch-feb2019-3.amxd` | **Clock → Tempo**: `rtin`→`sync~` → `attrui tempo` → `transport`; zweite Hälfte Kette → `bpmo` | 3600/1970/30 bzw. 2000/1866.67/133.33 | beide (sauberste Vorlage „externe Clock → eigenes Tempo") |
| `bpmoptisch-feb2019-midinotein.amxd` | wie ‑3, zusätzlich **BPM aus Notenabständen** (`timer`, `mtof`·60) und MIDI-Clock-Weitergabe | 2000/1967/33, 1800/1770/30; `s bpm1` + `s bpmo` | beide (BPM aus Notenabständen) |
| `bpmopi2.amxd` | **Bild-Zweig**: `jit.movie` → `jit.rota` → `jit.matrix 128×128` → Zellensuche | Suchschleife `if f1<=f2 && f3==0`, Schrittweite `f2/10`, Abbruch bei 0.000005; `s bpmpitest` | App (Bild-Scan: Zelle + Suchschleife) |
| `Audiovisuellbpmsync.amxd` | zwei feste **Bildpunkte** + Farbtoleranz erkennen den Musterwechsel → Film/Sample/Transport | Sollfarben „cell 7 11 val 255 34 37 50"; `cycle~ 10`→`snapshot~`→`expr f1·f2/f3/60/f4`; `metro_row.js` **fehlt** | App (Pixelpaar + Toleranz = Phase) |
| `AVbeta2.amxd` | großer AV-Hub: Videoanalyse + **Tap-BPM** + 8-stimmige Samples | `expr (60000/(f1/(f2+1)))` (BPM aus Zeit/Anzahl), 8× `groove~ sound1`, `pow(2.,(Note+k)/(f2−f3))`; `list.js` **fehlt** | beide (Tap-Formel, Noten→Pitchrate) |
| `skalender_timestretch-onoff16.amxd` | **16-stimmiger Timestretch-Sampler**, tempo-/cents-gekoppelt | 16× `buffer~`+`groove~`+`gate~`; `expr ((f1+f4)·1200/(f2−f3))` (Cents); `60000/bpm·…` → `clbang` | Desk/App (mehrstimmiges Sample-Pitchen, Anz-/On-Off) |
| `bpmcalculmate.amxd` | leere M4L-Vorlage („Build your MIDI effect here") | `midiin`/`midiout`, ein paar Rechenobjekte | — (Stub) |

**Gesamtbild:** eine klare Reihe — ‑2 (Rechenkern) → ‑3 (Clock→Tempo) → ‑1 (größte
Fassung) → midinotein (+Noten); alle teilen die tan/atan-Entfaltung und
Konstantentripel mit f1+f3=f2. Vier kleine Ideen mit Substanz: tan/atan-Entfaltung,
±2000-Alias, Pixelzelle+Toleranz, Tap/Clocker-BPM. Unklar bleibt bei allen die
**Einheit** der Zahlen (Hz/Samples/ms) — nicht geraten. `sync~`, `groove~`,
`clocker` sind normale Max-8-Objekte; die js-Dateien (`list.js`, `metro_row.js`)
fehlen auf dem Stick.

## Gruppe 2: Lissajous / audiovisuelle Figuren / Kurven

| Patch | Zweck | Kern | Kandidat |
|---|---|---|---|
| `lissa-2020optic.amxd` | audiovisuelles Lissajous-Instrument: zwei cycle~-Stimmen (L/R) + zwei groove~-Sampleplayer → `plugout~`; Werte per `snapshot~` zurück in MIDI (makenote/noteout) und 8-Schritt-Quantisierer | Phasenkette `tan(f1·π/87)` → `f1·f2/104` → `(atan(f1)+π)·f2/π`; Fenstertest `if f1<f2<f3 then f4`; `if f1>0` → `expr f1·f2/64`; 8× `receive beats` → `qmbeats2` | **App (genau der Lissa-Tab)**: L/R-Zweig, eine Stimme frequenzversetzt, Abtastung als Wertequelle |
| `lissa-midiout.amxd` | gleiche Architektur, schlanker (3 noteout, `r midi`, Bus `qmbeats`) | identische Kette/Formeln | App (schlankere Referenz) |
| `funktionaudio.amxd` | **Zellen-Walker**: zwei function-Kurven + counter x/y laufen über ein Raster; Kurvenwert steuert das Schritt-Tempo | `counter`, `clocker`, `if $i1==$i2 then stop`, `expr (60000/f1/f2·f3)` mit pow-2-Faktoren, `pack getcell 0 0` | Desk (Zellkoordinate → OSC/CSV; Kurve als Taktgeber) |
| `funktionaudio2.amxd` | Klon für andere Zell-Bus (`cellc` statt `cellc-3`) | wie oben | — (Duplikat) |
| `funktionaudio3.amxd` | + zweiter Zell-Bus (cell4x/cell4y) mit x/y-Offsets; **JSON defekt** (ein Array-Element ohne Anführungszeichen) | wie oben + `s count`, `xplus`/`yplus` | Desk (mehrere Stimmen/Bildbereiche) |
| `funktionaudio3-3.amxd` | größte Ausbaustufe: + Vergleichsblock (`expr ($i1)`, `if $i1==$i2 then bang`) und Bussen xchange/ychange | „nur bei Änderung senden" (Deduplizierung) | Desk (OSC/CSV-Deduplizierung) |
| `funktionaudio-countspiral.amxd` | Zähl-Baustein für die Spirale: empfängt Zellkoordinaten (`cell2`) und Knopfwerte | `unpack sym 0 0` → Gates; `sprlstep`-Busse; `start+knopfgraph`/`stop+knopfgraph` | Desk (rrn5: Spiral-Schritt) |
| `graphmixer.amxd` | **Kurven-Mischer**: x → Winkel 2π·x/128 → sin/cos/tan/linear/pow in 128er-Tabellen (itable) | `expr (6.283186·f1/128)`, `expr (sin|cos|tan(f1)·f2+f3)`, `expr $s2[$i1]`, `symbol tab1/tab2` | beide (Werte-Mapper für Figur/Phase und Tabellenkurven) |

**Gesamtbild:** zwei Linien — Oszillator-Abtastung mit Phasenkette (lissa-*) und
Raster-/Zellenmaschinen mit function-Kurven (funktionaudio-Kopierkette
→3 →3-3). `graphmixer` ist der allgemeine Werte-Mapper. Altlast: eingefrorene
Restwerte, defekte JSON in `funktionaudio3.amxd`; die Gegenstellen fehlen
(`clocker`-Abstraktion, die cell*-Busse werden von Fremdgeräten weitergeleitet).

## Gruppe 3: Spiralen / Linienbox-Renderer

| Patch | Zweck | Kern | Kandidat |
|---|---|---|---|
| `spiralobpm.amxd` | Spirale läuft durchs Zellraster, Radius → Note; Takt aus Transport-Tempo **oder** optischem `bpmo` | Archimedes `f2·f1·cos/sin(f1)` bzw. `ln(f2·f1)·cos/sin`; Zelladresse `pack getcell 0 0` → `s cellc`/`cellcobpm`; `sqrt` → `expr (f1·f3/f2+f4)` = Note; `1/(n·π)`-Reihe | beide (rrn5: Bild→Noten über Spiralpfad; App: Spirale über Harmoniekreis) |
| `Matrizenleser_Spiral_archimedisch.amxd` | schlankere Leser-Fassung ohne bpmo/cellcobpm-Ballast | identische Formeln | beide (schlankere Referenz) |
| `linboxrender-12.amxd` (+`-2`, `-3`) | **Linearbox-Renderer**: je eine Rampe über x- und y-Zellen; Meldet Zellkoordinaten | `gen`-Subpatch: counter → `!-` → `scale` (0…Zellen−1); `expr (60000/f1/f2·f3/f4)` = Dauer in ms; `s cellc`, Cursor `cellxset/cellyset` | beide (Wiper über Raster; in Rust simple Interpolation) |
| `linboxrender-1-knopf.amxd` | vereinfachter Renderer: zwei counter, per clocker und `xplus2`/`yplus2` getaktet | `metro @interval 1 ticks` → `transport` → `s bpm2`; alle Busse mit Suffix „2" | Desk (schrittweiser Zellen-Durchlauf) |
| `linboxrender-bild.amxd` | Bild-Variante: gezählte Zellen + Vorspul-Clocker + Kopplung an optisches BPM und zweites Raster | `transport` 2. Outlet → `if $i1==$i2 then bang` (Parameterwechsel **an festen Beats**); `s cell4x/cell4y`, `s bpmosek` | beide (Parameterwechsel auf Beat, ms pro Zelle) |
| `linboxrender-bild.maxpat.amxd` | erweiterte Fassung (live.dial xminus/yminus, zusätzlicher counter) | wie oben + Automatik `obj-209` | beide (vollständigere Fassung) |

**Gesamtbild:** alles sind **Raster-Adressierer** — sie erzeugen Zellkoordinaten
(„getcell x y") über gemeinsame Busse; das eigentliche Raster ist ein
`jit.matrix 4 char` in `dr22.amxd.maxpat` (Gruppe 5). Taktkern immer
`60000/BPM` × Faktoren (xmal/xdurch/xtelbar) → `clocker`/`counter`, gespeist aus
dem `transport`-Tempo oder `bpmo`. Die Spirale ist billige Mathematik
(r = a·φ bzw. r = ln(a·φ), x = r·cos φ, y = r·sin φ, φ per `accum`), Note aus
Radius-Normierung `sqrt/sqrtmax`. Für App/Desktop ist nur der Rechenkern
interessant; `transport`/`clocker`/`getcell` ersetzt man durch Link/BPM-Zeitbasis
und direkten Pixelzugriff. Die drei ‑12-Dateien sind bis auf gespeicherte Werte
identisch. Empfänger für `Note`, `celld`, `xz`, `yz`, `xmsek`, `ymsek`,
`xminus`, `yminus` liegen nicht im Ordner (vermutlich Live-Set).

## Gruppe 4: Noten / Chords / Pattern

| Patch | Zweck | Kern | Kandidat |
|---|---|---|---|
| `noteplay.amxd` (= `noteplay-2`) | MIDI-Notenspieler: Trigger + y/ys → Tonhöhe; 5 `makenote`→`noteout`, Stimmen über Register-Fenster freigeschaltet | `expr (f1·F2/f3)` (Trigger × E ÷ ys) → `+ TRP` → `s pitch`; Fenster `>=`/`<=` + `&&` (Schwellen 393/394/438/476); Vel aus `wattl`/`wattr`, Dauer aus Dial | beide (Wert→Tonhöhe, Vel/Dauer/Transpose, Register-Voices) |
| `noteplay-1.amxd` | ältere Fassung (nur E + TRP, Default-Note 45.23) | wie oben, weniger Boxen | — (Versionsstand) |
| `noteplay-3.amxd` | + drei **tempo-synchrone Auto-Stimmen** mit Zellauswahl | 3× `metro`+`counter` → Pitch-Messages → `makenote`; `r bpm`/`bpmo` → `expr (60000/f1/f2·f3)` → `clocker`; `r cellc` → `unpack sym 0 0` → `if f1>=f2 && f1<f3` | beide (Zellauswahl + Auto-Muster; Tempo-Formel) |
| `noteplay-3-save.amxd` / `-save2.amxd` | Zwischenstände: `notein` + ausgelagerter Stimmen-Subpatcher (pitch/slide/metro ms/res) | save: 699 Boxen, 16 metro, 23 makenote, `gen` entfernt | Desk (nur Referenz: parametrierter Subpatcher) |
| `mollchords.amxd` | **Moll-Akkord-Erkennung** (12 Namen) aus bis zu 6 klingenden Noten; MIDI läuft durch | 18 `if`-Boxen mit exakten Intervallvergleichen (`f2==f1+3 && f3==f1+7 && f4==f1+10`, ±12-Varianten) → Moll, m6, m7, m7/b5, m7/9, m6/9, madd9/11, mmaj7… | App (Intervalltabellen als Akkordname im Instrument-Tab); **„1if"-Tippfehler in 11 Boxen prüfen** |
| `showchord.amxd` | **Dur- und Moll-Erkennung** (31 Namen) + 6-Stimmen-Spieler + LCD-Farbe | 94 `if`-Boxen; `gen~` prüft Dur-Intervalle → `lcd` (`brgb`); `clocker`→`counter` → `s mpl`/`mpln`; `r nchXv` → Gates → `s nch1..6` | App (vollständigere Tabelle: sus/add/7er-Familie) |
| `2nd_3rd.maxpat` | Taktgeber-Bank: je 6 Notenwert-Metronome für 3 Ausgänge, nur wenn ein Wert im Bereich liegt | 3 Bänke × `metro 1n…32n`, `if $i1>=$i2 && $i1<=$i3` auf `r 2nd`, 24 live.dials | beide (bereichsgesteuerte Notenwert-Clocks) |
| `createpattern.maxpat` | sammelt Clock-Bangs als Textzeilen mit Schritt-Zählern, Bus `textline` | `r 3rd/3rd2/3rd3` → Gates → `counter` → `s textline` + `line $1` → `textedit` | Desk (Muster des Text-/CSV-Exports) |
| `pattern.maxpat` | **3-spuriger Step-Sequencer/-Recorder** auf `jit.cellblock` (rows 128) mit MIDI-Aufnahme und `noteout`-Wiedergabe | `metro` (Intervall `expr (60000/f1·f2/f3)`) → `counter` (Spalte/Zeile) → `pack select 0 0` → `noteout`; Aufnahme `r midi` → `pak set 0 0 0 0` | beide (direkteste Vorlage für pattern_recorder und das Rhythmus-Raster) |
| `eucl-midi-in out.amxd` | **Euklid-Verteiler**: 0/1-Muster aus `textedit` → 38 Bits → 18 Kanäle (MIDI, Audio, Zell-Bus) zeitlich auf-/zugeschaltet | `unjoin 38`; Fenster `if etmc>=etm && etmc<etm·(k+1)` UND `if bit==1`; `etm = expr (60000/f1/f2/f3·f4)`; Presets 5/7/9/17 Schritte | beide (kompakte Vorlage für Raster und OSC/MIDI-Verteilung); Fensterlogik = **Gate-Dauer statt Einzel-Step**, prüfen |
| `presenter.amxd` | **Fernbedienung**: ~60 Buttons/Toggles lösen per send-Namen andere Geräte aus | 8 `live.toggle` als Gruppen-Gates, `del 2` als Impulsformer, `s pr*` | — (systemgebunden; höchstens UI-Vorbild) |

**Gesamtbild:** fünf Familien — (1) `noteplay`-Reihe (Wert→Tonhöhe,
Vel/Dauer/Transpose, Register-Voices; ‑3 mit Auto-Mustern, save/save2 als
Zwischenstände), (2) Akkord-Erkennung (`mollchords` 12 Namen → `showchord`
31 Namen + LCD), (3) Clock-/Pattern-Kette `2nd_3rd` → `createpattern` →
`pattern` (jit.cellblock-Raster mit Aufnahme), (4) Euklid-Verteiler, (5)
`presenter` (Fernbedienung). Interessanteste Ergänzungen gegenüber dem, was
JamCore schon hat: Akkord-Namenstabellen, euklidisches 0/1-Muster mit
Kanal-Gating, bereichsgesteuerte Notenwert-Clocks, Grid-Recording
(`pattern.maxpat`). Nicht portieren: `presenter`, die save-Zwischenstände.

## Gruppe 5: Bild / Video / Render

| Patch | Zweck | Kern | Kandidat |
|---|---|---|---|
| `bild.amxd` | **8 Bild-Slots**, pro Takt umgeschaltet; `dropfile` lädt Bilder | `metro 4n`+`transport` → `if $i1==1..8 then bang` → `s pic1..pic8`; `fpic` (@autofit) zeigt; `r bildbang` von außen | beide (8-Slot-Umschaltung für Bild/Maske und Präsentation) |
| `keyselectbild.amxd` | **Note → Bild**: jedem Slot eine Note zugeordnet; bei der Note wechselt das Bild und 6 Wertekanäle werden frei | `kslider`→Gates; `if $i1==$i2 then 1..8` → `s bildbang`; `if $i1==0 then 0 else 1` (Note-off-Filter) | beide („Note spielen → Bild zeigen", 6 Kanäle je Bild) |
| `dr2.amxd.maxpat` | pixelgesteuertes M4L-Gerät: Kamera/Movie/8 Slots → `jit.rota` → `jit.matrix 4 char`; Zellen werden gelesen | `pack getcell 0 0` → `cell x y val a r g b` → `unpack sym 0 0 val 0 0 0 0` → `s fr/fg/fb`, `s x/y`; `makenote`/`noteout`, Vel/Dauer-Dials; 1272 Boxen mit Presentation-Layout | beide (**Pixel→RGB→Noten** = rrn5/Bild-Scan; Jitter/Export sind Max-spezifisch) |
| `dr22.amxd.maxpat` (+`-2`, `-3`) | dieselbe Engine, schlanker, mit sichtbarem `jit.window`: **Mausklick ins Bild setzt die Zelle** | `jit.window`→`route mouse mouseidle`→`unpack …`→`pack getcell`; `jit.grab`×2, `jit.movie`, `jit.record`, `s dimx/dimy`; `-3` mit Suffix-Bussen für Parallelbetrieb | beide (Klick ins Bild als Zellwahl; `jit.window` selbst nicht portierbar) |
| `onepatchflimmernrendern.amxd.maxpat` | Sammelpatch: dr2-Engine + keyselect-Kanäle + **Akkorderkennung** + Desktop-Capture + **Flimmer-Rendering** auf 256×256-lcd | 3 Pixelmodule, `jit.grab`×4, `jit.matrix`×5, `lcd` (`setpixel`, `writepict` mit RGB-Bereichen — Semantik **unbelegt**), `exportimage/exportmovie` | beide (Akkorderkennung kompakt übernehmbar; Flimmer-lcd/Jitter sind Max-spezifisch) |
| `Worm~.amxd` | M4L-Synth-Wrapper; Engine (`bpatcher worm.maxpat`) **fehlt auf dem Stick** | `live.tab` → `sel` → `offset` an bpatcher; `plugout~` | — (keine belegbare Logik) |
| `REEECHHH.maxpat` | Zähl-/Prüflogik ohne verdrahteten Ausgang (Fragment) | `qmetro 20` → counter, `expr ($i1-$i2*$i3-$i4*$i5) == 0` | — (Zweck unbelegt) |
| `deskrecord.maxpat` | **Bildschirmrecorder**: `jit.desktop` → Vorschau + `jit.record` (h264/jpeg) | `rec_cmd`-Steuerung, `mousestate`-Abfrage, `s desk2` | Desk (Idee wie rrn5s X11-Capture; Umsetzung Max-spezifisch) |
| `87654ert678.maxpat` | Jitter-Lehrstück: benannte 5×5-Matrix, Zellen per `setcell`/`getcell` | `jit.matrix this 4 char 5 5`, `pak setcell/getcell`, `plane $1` | Desk (Konzept Zellraster als Datenmodell) |
| `bechneiderdurch.amxd` | MIDI-Durchschleifer mit **BPM-synchronem Gate** (Stutter) | `midiin`→`gate`→`midiout`; `metro 1n`, Intervall `expr (60000/f1/f2·f3)`; `r bpm` | Desk (pattern_recorder: kleines Stutter/Stotter-Gate) |
| `fktscalerender.amxd` | **Funktionskurven → Zellen/Noten**: zwei `function`-Editoren (2093 Kurvenpunkte), Auswertung → Zellkoordinaten + Notenausgabe | `function` + `pack range/domain`; `getcell 14 78` → `s cellc2`; `counter`/`clocker`/`accum`/`random`; `expr ($f1·$f2/$f3)` und `(60000/f1/f2·f3)`; Kopplung an `prgraphsc*` (presenter) | beide („Kurve zeichnen → Schritte/Zellen") |

**Gesamtbild:** Die stärkste übertragbare Idee ist **„Bild/Video → Zelle (x,y) →
RGB → Noten"** (dr2/dr22/onepatch) samt Klick ins Bild als Zellwahl — das ist
inhaltlich rrn5 und der Bild-Scan der App; portierbar ist das Datenmodell, nicht
die Jitter-Objekte. Ebenfalls direkt übernehmbar: die Akkorderkennung aus 6
Notenkanälen und der 8-Bild-Slot-Umschalter mit „Note → Bild". Max-spezifisch
bleiben `jit.grab/movie/window`, der lcd-Flimmer, `jit.record`/`export*` und
`deskrecord`. `Worm~` (Engine fehlt), `REEECHHH` (unbelegt) und `87654ert678`
(Demo) tragen nichts bei. **Fehlende Fremdteile** auf dem Stick: `clocker`-Abstraktion,
`list.js`, `metro_row.js`, `worm.maxpat`, die Medien (dozer.mov, A/M/Y.png,
cr1/cr2/cr7.png) — die Patches laufen ohne sie nicht.

## Auswahlvorschläge

### Für die App (JamCore)

| Prio | Feature | Quelle | Aufwand |
|---|---|---|---|
| 1 | **Akkordname** aus den klingenden Noten/Notenbereichen (31 Namen: Dur/Moll, sus/add, 6er, 7er, maj7-Familie) | `showchord` + `mollchords` | klein (reine Intervallvergleiche im Kern + Anzeige im Instrument-Tab) |
| 2 | **Euklidisches Muster**: 0/1-Folge (Text) → Schritte im Rhythmus-Raster, Verteilung auf Kanäle/OSC | `eucl-midi-in out` | mittel (Muster-Generator im Kern, Raster-Anzeige) |
| 3 | **Note → Bild**: Noten (OSC/Instrument) schalten Bild-Slots um (8 Slots, 6 Wertekanäle je Slot) | `bild` + `keyselectbild` | mittel (Bild-Slots + Zuordnung, nutzt vorhandenen Bild-Panel-Zustand) |
| 4 | **Bereichsgesteuerte Notenwert-Clocks**: eine Note/Stimme nur, wenn ein Wert im Bereich liegt (6 Notenwerte) | `2nd_3rd` | klein |
| 5 | **Spiral-Scan** als zweite Bild-Abtastung: r = a·φ bzw. r = ln(a·φ), x = r·cos φ, y = r·sin φ, Radius → Tonhöhe (sqrt/sqrtmax) | `spiralobpm` / `Matrizenleser_Spiral_archimedisch` | mittel (reine Mathematik, ersetzt den Linien-Sweep) |
| 6 | **Klick ins Bild = Zellwahl** (statt nur Linien-Scan) | `dr22` (Idee) | klein |
| 7 | **Tap-BPM-Formel** 60000/(Zeit/Anzahl) als Gegenprobe zum vorhandenen Tap | `AVbeta2` | trivial |
| 8 | **Timestretch/Pitch (Cents) für den Player** mit Tempo-Folgen | `skalender_timestretch-onoff16` | groß (eigener Player-Zweig) |

Nicht empfohlen für die App: `presenter` (Fernbedienung), `noteplay-3-save*`
(Zwischenstände), `funktionaudio2` (Duplikat), `bpmcalculmate` (Stub),
`Worm~`, `REEECHHH`, `87654ert678`.

### Für die Desktop-Tools

| Prio | Feature | Quelle | Ziel |
|---|---|---|---|
| 1 | **Spiral-Scan** über das Bild (Archimedes/Logarithmus) mit Radius→Note | `spiralobpm`, `Matrizenleser_Spiral_archimedisch` | rrn5 (neben dem Linien-Sweep) |
| 2 | **Kurve zeichnen → Schritte/Zellen** (function-Editor → Raster/Noten) | `fktscalerender`, `funktionaudio` | pattern_recorder |
| 3 | **3-Spur-Grid-Recorder** (Zelle = Zeile/Spalte, Transpose je Spur, Aufnahme+Wiedergabe) | `pattern.maxpat` | pattern_recorder |
| 4 | **BPM-synchrones Gate/Stutter** (`metro 1n` + `gate`, Teiler/Multiplikator) | `bechneiderdurch` | pattern_recorder |
| 5 | **Bereichsgesteuerte Notenwert-Clocks** | `2nd_3rd` | pattern_recorder / rrn5 |
| 6 | **Werte-Mapper mit Kurven-Tabellen** (sin/cos/tan/linear/pow über 128 Zellen) | `graphmixer` | beide |
| 7 | **BPM aus Notenabständen** (`timer`, Note→BPM) | `bpmoptisch-feb2019-midinotein` | beide (Renoise-Noten) |
| 8 | **MIDI-Clock rein/raus** als Tempoquelle/-senke | `bpmoptisch-feb2019` (‑1/‑3) | Desktop (Link gibt es nur in der App) |
| 9 | **Zellkoordinaten als OSC/CSV** mit „nur bei Änderung senden" | `funktionaudio3-3` | beide (Deduplizierung) |

Nicht empfohlen für den Desktop: `presenter`, die `-2`/`-3`-Kopien der
`linboxrender`/`dr22`-Reihe (bis auf gespeicherte Werte identisch),
`bpmoptisch-feb2019-2` (Referenz), `bpmcalculmate`, `Worm~`, `REEECHHH`,
`87654ert678`, `deskrecord` (Idee steckt schon in rrn5s X11-Capture).

### Offene Punkte vor einer Übernahme

- `mollchords`: 11 Boxen heißen `1if` (vermutlich Tippfehler → Erkennungen wirkungslos).
- `eucl-midi-in out`: das Gate ist eine **Dauer** (Kanal k offen während der Schritte 1…k), kein Einzel-Step.
- `bpmoptisch-*`: die **Einheit** der Zahlen (Hz/Samples/ms) ist nirgends belegt.
- `funktionaudio3.amxd`: JSON defekt (ein Array-Element ohne Anführungszeichen) – Original unberührt.
- `onepatch…`: `writepict`-RGB-Bereichsargumente unbelegt (Standard-lcd kennt sie nicht).
