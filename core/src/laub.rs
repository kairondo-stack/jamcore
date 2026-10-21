//! Laub-Rhythmus: Blätter auf einem Weg finden und ihre Abstände in einen
//! Genre-Rhythmus übersetzen (Nutzeridee 2026-10-18, Plan
//! `docs/plans/2026-10-18-laub-rhythmus.md`).
//!
//! Alles hardwarefrei, damit Desktop (Foto laden) und Android (Kamera)
//! dieselbe Rechnung sehen. Zwei Schritte:
//!
//! 1. **[`blaetter`]** findet auf einem Bild (RGB8) die Flecken, die sich vom
//!    Weg (Hintergrundfarbe) abheben: Farbschwelle und
//!    Zusammenhangskomponenten, kein ML. Jedes Blatt trägt Mittelpunkt,
//!    Fläche und Mittelfarbe; [`farbklasse`] macht daraus rot/gelb/braun/grün
//!    (in der App je Klasse ein Instrument).
//! 2. **[`schritte_aus_abstaenden`]** macht aus den Abständen entlang des Wegs
//!    Schrittnummern im Raster ([`rasterwahl`]: 96/128/192/256 Schritte pro
//!    Takt), gerundet auf das Vokabular eines Genres ([`genre`]). Das Ergebnis
//!    liegt in `0..raster_pro_takt` und wiederholt sich mit jedem Takt.
//!
//! # Raster und Vokabeln
//!
//! Ein Takt ist 4/4 mit `raster_pro_takt` Schritten; bei 96 Schritten ist eine
//! Viertel 24, ein Achtel 12, ein 16tel 6 und ein 32stel 3 Schritte. Die
//! Vokabeln der Genres stehen in Schritten des **Referenzrasters 96** und
//! werden auf das gewählte Raster skaliert (bei 192 also verdoppelt, bei 128
//! mal 4/3).
//!
//! Die **erste** Vokabel ist der *Grundschritt*: der Puls des Genres, mit dem
//! der relative Modus den kleinsten Abstand gleichsetzt. Gerundet wird auf
//! jedes Vielfache **jeder** Vokabel — ein doppelt so großer Abstand wird so
//! auch ein doppelt so großer Schritt.
//!
//! # Abstandsmodi
//!
//! - [`Abstandsmodus::Relativ`]: nur die Verhältnisse zählen. Der kleinste
//!   Abstand ist genau ein Grundschritt, die anderen im Verhältnis dazu, dann
//!   auf das Vokabular gerundet. Für Fotos, bei denen kein Maßstab bekannt ist.
//! - [`Abstandsmodus::Absolut`]: Physik. Abstand (Meter) geteilt durch die
//!   Gehgeschwindigkeit gibt Sekunden; Sekunden × BPM × Raster / 240 gibt
//!   Schritte. Die BPM kommen vom Aufrufer ([`schritte_aus_abstaenden_bpm`])
//!   oder sind [`BPM_VORGABE`] (120) — das Starttempo der übrigen Werkzeuge
//!   und ein ruhiger Mittelfall für einen Geh-Rhythmus.
//!
//! # Rechenbeispiel
//!
//! Fünf Abstände, 1,2 m/s Gehgeschwindigkeit, 120 BPM, Raster 96, Genre
//! Techno: 1,2 m → 48 Schritte, 1,8 m → 72, 1,2 m → 48, 2,4 m → 96 (ein
//! ganzer Takt, die Position wiederholt sich), 1,2 m → 48. Aufsummiert und im
//! Takt gefaltet ergibt das 0, 48, 24, 72, 72, 24 — sortiert und ohne
//! Doppelte **0, 24, 48, 72**, also ein sauberes 4-on-the-floor.

/// Mindestfläche eines Fleckens in Bildpunkten. Kleineres ist Rauschen (ein
/// Störpixel, ein Kratzer) und kein Blatt.
pub const MIN_FLAECHE: u32 = 4;

/// Farbklasse 0: rot (auch dunkles Rot).
pub const FARBE_ROT: u8 = 0;
/// Farbklasse 1: gelb (helles Orange/Gelb).
pub const FARBE_GELB: u8 = 1;
/// Farbklasse 2: braun (dunkles Warmbraun, Grau, Beige).
pub const FARBE_BRAUN: u8 = 2;
/// Farbklasse 3: grün.
pub const FARBE_GRUEN: u8 = 3;

/// Formklasse 0: rund (quadratische Ausdehnung, gut gefüllt).
pub const FORM_RUND: u8 = 0;
/// Formklasse 1: länglich (deutlich breiter als hoch oder umgekehrt).
pub const FORM_LAENGLICH: u8 = 1;
/// Formklasse 2: gezackt/ausgefranst (füllt die Bounding-Box schlecht).
pub const FORM_GEZACKT: u8 = 2;

/// Seitenverhältnis ab dem ein Blatt als länglich gilt: 2:1. Ein
/// Ahornblatt ist eher rundlich, ein Weidenblatt deutlich länger.
const FORM_VERHAELTNIS_LAENGLICH: f32 = 2.0;

/// Füllgrad der Bounding-Box, ab dem ein Blatt als gezackt gilt: höchstens
/// 55 %. Ein Kreis füllt 78 %, ein gezacktes Blatt deutlich weniger.
const FORM_FUELLGRAD_GEZACKT: f32 = 0.55;

/// Vorgabe-Tempo für den absoluten Modus, wenn der Aufrufer keins mitgibt:
/// 120 BPM. Das ist das Starttempo der übrigen Werkzeuge (Mic-BPM-Tab) und
/// ein ruhiger Mittelfall für einen Geh-Rhythmus.
pub const BPM_VORGABE: f32 = 120.0;

/// Vorgabe-Gehgeschwindigkeit in Metern je Sekunde: 1,2 m/s ≈ 4,3 km/h, ein
/// gemütliches Gehen auf einem Weg.
pub const GEHTEMPO_M_S: f32 = 1.2;

/// Vorgabe-Schrittlänge in Metern: 0,75 m, der normale Schritt eines
/// Erwachsenen.
pub const SCHRITTLANGE_M: f32 = 0.75;

/// Die wählbaren Raster: Schritte pro Takt (4/4). Bei 96 Schritten ist eine
/// Viertel 24 Schritte, bei 256 eine Viertel 64; die Patternlänge der App
/// (bis 1024 Schritte) fasst damit bis zu vier Takte.
pub fn rasterwahl() -> [u16; 4] {
    [96, 128, 192, 256]
}

/// Ein gefundenes Blatt: Mittelpunkt in Pixelkoordinaten (Spalte, Zeile),
/// Fläche in Bildpunkten und die Mittelfarbe des Fleckens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blatt {
    /// Spalte des Flächenschwerpunkts (0 = linke Bildkante).
    pub x: f32,
    /// Zeile des Flächenschwerpunkts (0 = obere Bildkante).
    pub y: f32,
    /// Anzahl Bildpunkte des Fleckens (mindestens [`MIN_FLAECHE`]).
    pub flaeche: u32,
    /// Breite der Bounding-Box in Bildpunkten (größte Spalte − kleinste
    /// Spalte + 1). Daraus kommt die Form ([`formklasse`]).
    pub breite: u32,
    /// Höhe der Bounding-Box in Bildpunkten.
    pub hoehe: u32,
    /// Mittlerer Rotanteil des Fleckens.
    pub r: u8,
    /// Mittlerer Grünanteil des Fleckens.
    pub g: u8,
    /// Mittlerer Blauanteil des Fleckens.
    pub b: u8,
}

/// Filter für die Blatterkennung: Fläche (Blattgröße), Sättigung und
/// Helligkeit. Die Vorgabe ist **neutral** (nur [`MIN_FLAECHE`]) und liefert
/// deshalb dieselben Blätter wie [`blaetter`] — die Filter sind eine
/// freiwillige Verengung für schwierige Bilder (Moos, Kies, Schatten), keine
/// stille Änderung der bisherigen Erkennung.
///
/// Warum überhaupt filtern: Auf einem Weg liegen neben Blättern auch Steine,
/// Kies und Schatten. Jeder Filter nimmt eine Gruppe möglicher Störer weg;
/// mehrere Filter zusammen sind die „größere Menge an Ausschlussmöglichkeiten“
/// aus dem Diktat (2026-10-20), die der Nutzer dann fein einstellt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blattfilter {
    /// Kleinste Fläche in Bildpunkten (mindestens [`MIN_FLAECHE`]).
    pub min_flaeche: u32,
    /// Größte Fläche in Bildpunkten; `0` heißt **keine** Obergrenze (Vorgabe).
    pub max_flaeche: u32,
    /// Kleinste Sättigung (0…1) der Mittelfarbe. Grau/Beige hat fast keine
    /// Sättigung und fällt damit weg, wenn man nur bunte Blätter will.
    pub min_saettigung: f32,
    /// Kleinste Helligkeit 0…255 (größter Farbkanal der Mittelfarbe).
    pub min_helligkeit: u8,
    /// Größte Helligkeit 0…255.
    pub max_helligkeit: u8,
}

impl Default for Blattfilter {
    fn default() -> Self {
        Self {
            min_flaeche: MIN_FLAECHE,
            max_flaeche: 0,
            min_saettigung: 0.0,
            min_helligkeit: 0,
            max_helligkeit: 255,
        }
    }
}

impl Blattfilter {
    /// Ob ein gefundenes Blatt den Filter besteht. Kaputte Einstellungen
    /// (Sättigung außerhalb 0…1, Helligkeit min > max) werden geklemmt bzw.
    /// getauscht, statt still alles oder nichts zu liefern.
    pub fn behaelt(&self, blatt: &Blatt) -> bool {
        if blatt.flaeche < self.min_flaeche.max(MIN_FLAECHE) {
            return false;
        }
        if self.max_flaeche > 0 && blatt.flaeche > self.max_flaeche {
            return false;
        }
        let (_, s, _) = hsv(blatt.r, blatt.g, blatt.b);
        if s < self.min_saettigung.clamp(0.0, 1.0) {
            return false;
        }
        let (min, max) = helligkeitsbereich(self.min_helligkeit, self.max_helligkeit);
        let v = blatt.r.max(blatt.g).max(blatt.b);
        (min..=max).contains(&v)
    }
}

/// Helligkeitsbereich als (min, max), bei verdrehten Werten getauscht.
fn helligkeitsbereich(min: u8, max: u8) -> (u8, u8) {
    if min <= max {
        (min, max)
    } else {
        (max, min)
    }
}


/// Alle Blätter in einem Bild.
///
/// `frame` ist RGB8 (drei Bytes je Bildpunkt, zeilenweise, wie es die
/// Kamera-Brücke nach `yuv` liefert), `w` und `h` sind Breite und Höhe in
/// Bildpunkten. Ein Bildpunkt gehört zum Vordergrund, wenn er sich in
/// **einem** Kanal um mehr als `toleranz` von `hintergrund` unterscheidet (es
/// zählt der größte Kanalabstand). Zusammenhängende Vordergrundpunkte bilden
/// einen Flecken; die Nachbarschaft ist die 8er, diagonal berührte Punkte
/// gehören also zusammen (ein Blatt ist ein kompakter Flecken, keine
/// Vierer-Kreuzung). Flecken ab [`MIN_FLAECHE`] Bildpunkten werden ein
/// [`Blatt`], kleinere sind Rauschen.
///
/// Kaputte Werte sind kein Fehler: `w`/`h` ≤ 0 oder ein zu kurzer `frame`
/// ergeben eine leere Liste. Die Reihenfolge ist die des Bildlaufs (Zeile für
/// Zeile, je Flecken der erste Punkt); für die Wegrichtung danach
/// [`sortiere_wegrichtung`] aufrufen.
///
/// Für Handy-Fotos vorher verkleinern: die Vordergrundmaske braucht ein Byte
/// je Bildpunkt.
pub fn blaetter(
    frame: &[u8],
    w: i32,
    h: i32,
    hintergrund: (u8, u8, u8),
    toleranz: u8,
) -> Vec<Blatt> {
    blaetter_mit_filter(
        frame,
        w,
        h,
        hintergrund,
        toleranz,
        &Blattfilter::default(),
    )
}

/// Wie [`blaetter`], aber mit einem [`Blattfilter`] (Fläche, Sättigung,
/// Helligkeit).
///
/// Die Blob-Erkennung ist dieselbe; gefiltert wird nach der Mittelfarbe des
/// fertigen Fleckens. Warum erst danach: Sättigung und Helligkeit sind
/// Mittelwerte über das ganze Blatt und damit stabiler als das erste Pixel;
/// die Mindestfläche wird zusätzlich schon beim Zählen angewandt, weil ein
/// Fleck unter [`MIN_FLAECHE`] nie ein Blatt ist.
pub fn blaetter_mit_filter(
    frame: &[u8],
    w: i32,
    h: i32,
    hintergrund: (u8, u8, u8),
    toleranz: u8,
    filter: &Blattfilter,
) -> Vec<Blatt> {
    if w <= 0 || h <= 0 {
        return Vec::new();
    }
    let (bw, bh) = (w as usize, h as usize);
    let Some(benoetigt) = bw.checked_mul(bh).and_then(|p| p.checked_mul(3)) else {
        return Vec::new();
    };
    if frame.len() < benoetigt {
        return Vec::new();
    }
    // Vordergrundmaske: ein Byte je Bildpunkt.
    let mut maske: Vec<bool> = Vec::with_capacity(bw * bh);
    for (i, p) in frame.chunks(3).enumerate() {
        if i >= bw * bh {
            break;
        }
        maske.push(ist_blattpixel((p[0], p[1], p[2]), hintergrund, toleranz));
    }
    let mut gesehen = vec![false; bw * bh];
    let mut stapel: Vec<usize> = Vec::new();
    let mut raus: Vec<Blatt> = Vec::new();
    for start in 0..bw * bh {
        if !maske[start] || gesehen[start] {
            continue;
        }
        gesehen[start] = true;
        stapel.push(start);
        let (mut flaeche, mut sx, mut sy, mut sr, mut sg, mut sb) =
            (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
        // Bounding-Box: kleinste/größte Spalte und Zeile des Fleckens. Daraus
        // kommt die Form (Seitenverhältnis, Füllgrad), nicht aus der Fläche.
        let (mut min_x, mut max_x) = (bw, 0usize);
        let (mut min_y, mut max_y) = (bh, 0usize);
        while let Some(i) = stapel.pop() {
            let (x, y) = (i % bw, i / bw);
            flaeche += 1;
            sx += x as u64;
            sy += y as u64;
            sr += frame[3 * i] as u64;
            sg += frame[3 * i + 1] as u64;
            sb += frame[3 * i + 2] as u64;
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
            // 8er-Nachbarschaft: auch die vier Diagonalen.
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= bw as i64 || ny >= bh as i64 {
                        continue;
                    }
                    let j = ny as usize * bw + nx as usize;
                    if maske[j] && !gesehen[j] {
                        gesehen[j] = true;
                        stapel.push(j);
                    }
                }
            }
        }
        if flaeche < MIN_FLAECHE as u64 {
            continue;
        }
        let n = flaeche as f32;
        let blatt = Blatt {
            x: sx as f32 / n,
            y: sy as f32 / n,
            flaeche: flaeche as u32,
            breite: (max_x - min_x + 1) as u32,
            hoehe: (max_y - min_y + 1) as u32,
            r: (sr as f32 / n).round() as u8,
            g: (sg as f32 / n).round() as u8,
            b: (sb as f32 / n).round() as u8,
        };
        if filter.behaelt(&blatt) {
            raus.push(blatt);
        }
    }
    raus
}

/// Ob ein Bildpunkt zum Blatt gehört: der größte Kanalabstand zur
/// Hintergrundfarbe ist größer als die Toleranz.
fn ist_blattpixel(p: (u8, u8, u8), hintergrund: (u8, u8, u8), toleranz: u8) -> bool {
    let abstand = |a: u8, b: u8| (a as i32 - b as i32).abs();
    abstand(p.0, hintergrund.0)
        .max(abstand(p.1, hintergrund.1))
        .max(abstand(p.2, hintergrund.2))
        > toleranz as i32
}

/// Blätter in Wegrichtung sortieren: erst nach `y` (Bildzeile), bei gleicher
/// Zeile nach `x` (Spalte).
///
/// Die Schrittnummern werden in dieser Reihenfolge aufsummiert, deshalb ist
/// die Sortierung die halbe Miete: der Weg läuft von der Kamera weg (oder auf
/// sie zu) — in beiden Fällen ist „erst y, dann x“ die Richtung, in der die
/// Blätter abgelaufen wurden.
pub fn sortiere_wegrichtung(blaetter: &mut [Blatt]) {
    blaetter.sort_by(|a, b| a.y.total_cmp(&b.y).then_with(|| a.x.total_cmp(&b.x)));
}

/// Farbklasse eines Blattes: 0 rot, 1 gelb, 2 braun, 3 grün (die Konstanten
/// [`FARBE_ROT`], [`FARBE_GELB`], [`FARBE_BRAUN`], [`FARBE_GRUEN`]).
///
/// Einfache HSV-Regeln, kein ML: Grau/Beige (Sättigung < 0,15) und dunkles
/// Warmbraun sind braun, der Farbton entscheidet sonst: unter 20° rot, unter
/// 70° gelb, unter 170° grün. Blau und Violett gibt es beim Laub nicht und
/// zählen als braun.
pub fn farbklasse(b: &Blatt) -> u8 {
    farbklasse_rgb(b.r, b.g, b.b)
}

/// Wie [`farbklasse`], aber direkt aus den Kanälen.
pub fn farbklasse_rgb(r: u8, g: u8, b: u8) -> u8 {
    let (h, s, v) = hsv(r, g, b);
    if s < 0.15 {
        return FARBE_BRAUN; // Grau/Beige: kein Farbton erkennbar
    }
    if h < 20.0 || h >= 335.0 {
        return FARBE_ROT;
    }
    if h < 70.0 {
        // Helles Orange/Gelb ist gelb, dunkles ist braun (Herbstlaub).
        return if v < 0.5 { FARBE_BRAUN } else { FARBE_GELB };
    }
    if h < 170.0 {
        return FARBE_GRUEN;
    }
    FARBE_BRAUN
}

/// Name der Farbklasse für die Anzeige („rot“, „gelb“, „braun“, „grün“);
/// unbekannte Klassen ergeben „?“.
pub fn farbklasse_name(klasse: u8) -> &'static str {
    match klasse {
        FARBE_ROT => "rot",
        FARBE_GELB => "gelb",
        FARBE_BRAUN => "braun",
        FARBE_GRUEN => "grün",
        _ => "?",
    }
}

/// HSV aus RGB: Farbton 0..360 (0 = rot), Sättigung und Helligkeit 0..1.
fn hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let spanne = max - min;
    let mut h = if spanne <= 1e-6 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / spanne) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / spanne + 2.0)
    } else {
        60.0 * ((r - g) / spanne + 4.0)
    };
    if h < 0.0 {
        h += 360.0;
    }
    let s = if max <= 1e-6 { 0.0 } else { spanne / max };
    (h, s, max)
}

/// Die vier **Farbmischwerte** eines Blattes: Zugehörigkeit zu rot, gelb,
/// braun und grün, jeweils 0…1 und zusammen 1 (Reihenfolge wie die
/// Farbklassen [`FARBE_ROT`], [`FARBE_GELB`], [`FARBE_BRAUN`],
/// [`FARBE_GRUEN`]).
///
/// # Warum Mischwerte statt fester Grenzen
///
/// Herbstlaub ist selten ein reines Rot oder Gelb: ein Blatt liegt zwischen
/// beiden, je nach Licht und Blattadern. [`farbklasse_rgb`] schneidet bei
/// genau 20° Farbton — ein Blatt bei 19,9° wäre rot, bei 20,1° gelb. Für den
/// Nutzer sind das **Abstände zwischen den genauen Farbwerten**; gewünscht
/// sind Abstände zwischen **Farbmischwerten** (Diktat 2026-10-20), damit
/// kleine Lichtänderungen die Klasse nicht umspringen lassen und mehr
/// Zwischenfälle beschreibbar sind.
///
/// Deshalb hier eine weiche Ähnlichkeit zu vier **Prototypen** im
/// HSV-Raum (Farbton, Sättigung, Helligkeit):
///
/// | Klasse | Farbton | Sättigung | Helligkeit |
/// |---|---|---|---|
/// | rot   | 0°   | 0,85 | 0,75 |
/// | gelb  | 50°  | 0,85 | 0,90 |
/// | braun | 30°  | 0,50 | 0,35 |
/// | grün  | 110° | 0,65 | 0,55 |
///
/// Der Abstand ist die Summe der quadrierten Unterschiede (Farbton in 60°-
/// Schritten, Sättigung und Helligkeit in 0,4-Schritten); das Gewicht ist
/// `1 / (1 + Abstand)`. Kein harter Schnitt: ein Rot-Gelb-Blatt teilt sein
/// Gewicht auf beide, ein graues Blatt liegt beim Braun-Prototyp (Sättigung
/// niedrig) und bleibt damit braun, ein sehr dunkles ebenso (Helligkeit
/// niedrig). Die Normierung auf Summe 1 macht die Werte untereinander
/// vergleichbar.
pub fn farbmischwerte(r: u8, g: u8, b: u8) -> [f32; 4] {
    let (h, s, v) = hsv(r, g, b);
    // (Farbton, Sättigung, Helligkeit) je Klasse, Reihenfolge wie die
    // Farbklassen. Braun sitzt bei einem dunklen, mäßig gesättigten Orange —
    // dort liegt Herbstlaub, das weder rot noch gelb noch grau ist.
    const PROTOTYPEN: [(f32, f32, f32); 4] = [
        (0.0, 0.85, 0.75),
        (50.0, 0.85, 0.90),
        (30.0, 0.50, 0.35),
        (110.0, 0.65, 0.55),
    ];
    let mut gewichte = [0.0f32; 4];
    let mut summe = 0.0f32;
    for (i, (ph, ps, pv)) in PROTOTYPEN.iter().enumerate() {
        // Farbton als Kreis: 350° und 10° sind 20° auseinander, nicht 340°.
        let dh = (h - ph).abs().min(360.0 - (h - ph).abs()) / 60.0;
        let ds = (s - ps) / 0.4;
        let dv = (v - pv) / 0.4;
        let abstand = dh * dh + ds * ds + dv * dv;
        let gewicht = 1.0 / (1.0 + abstand);
        gewichte[i] = gewicht;
        summe += gewicht;
    }
    if summe > 0.0 {
        for gewicht in &mut gewichte {
            *gewicht /= summe;
        }
    }
    gewichte
}

/// Farbklasse über **Mischwerte** statt exakter RGB-Grenzen: die Klasse mit
/// dem größten [`farbmischwerte`]-Gewicht, Rückgabe 0…3 wie [`farbklasse`].
///
/// Warum das für Gruppen und Instrumente reicht: Die Mischwerte entscheiden
/// weich (ein Rot-Gelb-Blatt kippt erst bei gleichem Abstand zu beiden, also
/// bei 25° statt bei 20°), die Klasse bleibt aber eine der vier
/// Instrumentklassen. Bei Gleichstand gewinnt die frühere Klasse der
/// Reihenfolge rot, gelb, braun, grün — damit ist das Ergebnis auch bei
/// exakt gleichen Gewichten eindeutig.
pub fn farbklasse_mix(r: u8, g: u8, b: u8) -> u8 {
    let gewichte = farbmischwerte(r, g, b);
    let mut beste = FARBE_ROT;
    let mut bester = gewichte[FARBE_ROT as usize];
    for klasse in [FARBE_GELB, FARBE_BRAUN, FARBE_GRUEN] {
        if gewichte[klasse as usize] > bester {
            bester = gewichte[klasse as usize];
            beste = klasse;
        }
    }
    beste
}

/// Wie [`farbklasse`], aber über die Mischwerte ([`farbklasse_mix`]).
pub fn farbklasse_blatt_mix(blatt: &Blatt) -> u8 {
    farbklasse_mix(blatt.r, blatt.g, blatt.b)
}

/// Formklasse eines Blattes: 0 rund, 1 länglich, 2 gezackt/ausgefranst (die
/// Konstanten [`FORM_RUND`], [`FORM_LAENGLICH`], [`FORM_GEZACKT`]).
///
/// # Warum Form-Mischwerte
///
/// Wie bei der Farbe (Diktat 2026-10-20) soll nicht ein einzelner genauer
/// Wert die Klasse bestimmen, sondern das **Gemisch** der Ausdehnung. Ein
/// Blatt ist selten exakt quadratisch; rund ist es, wenn Breite und Höhe
/// ähnlich sind **und** die Fläche die Bounding-Box gut füllt. Länglich ist
/// es ab 2:1 Seitenverhältnis (Weide, Esche). Bleibt bei fast quadratischer
/// Box zu wenig Fläche übrig (höchstens 55 %), ist der Rand gezackt oder
/// ausgefranst (Eiche, Ahorn) — die Fläche „zerfällt“ in der Box.
///
/// Reihenfolge der Prüfung: erst länglich (die Ausdehnung ist die stärkere
/// Aussage), dann gezackt, sonst rund. Ohne Fläche oder ohne Box gibt es
/// keine Form; dann ist das Ergebnis der neutrale Wert [`FORM_RUND`], damit
/// die Anzeige nichts Falsches behauptet.
pub fn formklasse(blatt: &Blatt) -> u8 {
    if blatt.flaeche == 0 || blatt.breite == 0 || blatt.hoehe == 0 {
        return FORM_RUND;
    }
    let (kurz, lang) = (
        blatt.breite.min(blatt.hoehe) as f32,
        blatt.breite.max(blatt.hoehe) as f32,
    );
    let verhaeltnis = lang / kurz;
    let fuellgrad = blatt.flaeche as f32 / (blatt.breite as f32 * blatt.hoehe as f32);
    if verhaeltnis >= FORM_VERHAELTNIS_LAENGLICH {
        FORM_LAENGLICH
    } else if fuellgrad <= FORM_FUELLGRAD_GEZACKT {
        FORM_GEZACKT
    } else {
        FORM_RUND
    }
}

/// Name der Formklasse für die Anzeige („rund“, „länglich“, „gezackt“);
/// unbekannte Klassen ergeben „?“.
pub fn formklasse_name(klasse: u8) -> &'static str {
    match klasse {
        FORM_RUND => "rund",
        FORM_LAENGLICH => "länglich",
        FORM_GEZACKT => "gezackt",
        _ => "?",
    }
}

/// Farbklasse eines Blattes je nach Modus: exakt ([`farbklasse`]) oder über
/// die Mischwerte ([`farbklasse_blatt_mix`]).
fn farbklasse_je(blatt: &Blatt, gemischt: bool) -> u8 {
    if gemischt {
        farbklasse_blatt_mix(blatt)
    } else {
        farbklasse(blatt)
    }
}

/// Klassen einer Gruppe nach Häufigkeit: `(Klasse, Anzahl)`, häufigste
/// zuerst, bei Gleichstand die kleinere Klasse (feste Reihenfolge, damit die
/// Beschreibung nicht bei jedem Aufruf anders aussieht).
fn haeufigste(klassen: impl Iterator<Item = u8>, anzahl: usize) -> Vec<(u8, usize)> {
    let mut zaehler = vec![0usize; anzahl];
    for klasse in klassen {
        if let Some(feld) = zaehler.get_mut(klasse as usize) {
            *feld += 1;
        }
    }
    let mut liste: Vec<(u8, usize)> = zaehler
        .into_iter()
        .enumerate()
        .filter(|(_, n)| *n > 0)
        .map(|(i, n)| (i as u8, n))
        .collect();
    liste.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    liste
}

/// Beschreibung einer Abstandsgruppe für den Laub-Tab:
/// `G1: 4 Schritte · rot-gemischt · rund (3 Blätter)`.
///
/// `nummer` ist 1-basiert (wie in der Anzeige), `schritte` die Zahl der
/// Schritte im Takt, `blaetter` die Blätter der Gruppe und `gemischt` der
/// Farbmodus. Die Farbe ist die häufigste Klasse der Gruppe; kommen mehrere
/// vor, stehen sie mit `/` getrennt da (häufigste zuerst) — das sind die
/// „Diversitäten“, nach deren Beschreibung das Diktat fragt. Die Form wird
/// genauso aus [`formklasse`] gezählt. Ohne Blätter bleibt die Zeile bei
/// Schritten und „ohne Blätter“.
pub fn beschreibe_gruppe(nummer: usize, schritte: usize, blaetter: &[Blatt], gemischt: bool) -> String {
    let mut text = format!("G{nummer}: {schritte} Schritte");
    if blaetter.is_empty() {
        text.push_str(" · ohne Blätter");
        return text;
    }
    let farben: Vec<&str> = haeufigste(blaetter.iter().map(|b| farbklasse_je(b, gemischt)), 4)
        .into_iter()
        .map(|(klasse, _)| farbklasse_name(klasse))
        .collect();
    let formen: Vec<&str> = haeufigste(blaetter.iter().map(formklasse), 3)
        .into_iter()
        .map(|(klasse, _)| formklasse_name(klasse))
        .collect();
    format!(
        "{text} · {}-{} · {} ({} Blätter)",
        farben.join("/"),
        if gemischt { "gemischt" } else { "exakt" },
        formen.join("/"),
        blaetter.len()
    )
}

/// Untergruppen einer Gruppe: die Blätter nach **Farb- und Form-Mix** zählen
/// und anzeigen, z. B.
/// `G1a: rot/gemischt · rund, 2 · G1b: gelb/gemischt · länglich, 1`.
///
/// Warum eigene Untergruppen: Die Gruppen sind Taktabstände; erst die
/// Aufteilung nach Farbe und Form zeigt, welche „Sorten“ in einem Takt
/// stecken (Diktat 2026-10-20: „Diversitäten in den Untergruppen“). Sortiert
/// wird nach Anzahl (häufigste zuerst), dann Farbe, dann Form — dieselbe
/// Eingabe ergibt dieselbe Beschreibung. Ohne Blätter ist die Zeile leer.
pub fn beschreibe_untergruppen(nummer: usize, blaetter: &[Blatt], gemischt: bool) -> String {
    // 4 Farben × 3 Formen: mehr Untergruppen kann es nicht geben.
    let mut zaehler = [[0usize; 3]; 4];
    for blatt in blaetter {
        let farbe = farbklasse_je(blatt, gemischt) as usize;
        let form = formklasse(blatt) as usize;
        if let Some(feld) = zaehler.get_mut(farbe).and_then(|zeile| zeile.get_mut(form)) {
            *feld += 1;
        }
    }
    let mut eintraege: Vec<(u8, u8, usize)> = zaehler
        .iter()
        .enumerate()
        .flat_map(|(farbe, zeile)| {
            zeile
                .iter()
                .enumerate()
                .filter(|(_, n)| **n > 0)
                .map(move |(form, n)| (farbe as u8, form as u8, *n))
        })
        .collect();
    eintraege.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
    let modus = if gemischt { "gemischt" } else { "exakt" };
    eintraege
        .iter()
        .enumerate()
        .map(|(i, (farbe, form, n))| {
            // Höchstens 12 Untergruppen (4 × 3), also reichen a…l.
            let buchstabe = char::from(b'a' + i as u8);
            format!(
                "G{nummer}{buchstabe}: {}/{modus} · {}, {n}",
                farbklasse_name(*farbe),
                formklasse_name(*form)
            )
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Ein Genre: Name und Vokabular typischer Schrittintervalle.
///
/// Die Werte stehen in Schritten des **Referenzrasters 96** (24 = Viertel,
/// 12 = Achtel, 6 = 16tel, 3 = 32stel) und werden auf das gewählte Raster
/// skaliert. Die **erste** Vokabel ist der Grundschritt (der Puls des
/// Genres), die weiteren sind typische Abweichungen und Splits; gerundet wird
/// auf jedes Vielfache jeder Vokabel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Genre {
    /// Anzeigename, z. B. „Techno“.
    pub name: &'static str,
    /// Typische Schrittintervalle im Referenzraster 96.
    pub vokabular: &'static [u32],
}

/// Techno: 4-on-the-floor. Eine Vokabel, die Viertel (`Raster/4`) — jeder
/// Blattabstand wird ein Vielfaches eines Viertels, der Puls liegt auf jeder
/// Viertel.
const VOKABULAR_TECHNO: &[u32] = &[24];
/// Groove: Achtel-Puls mit Swing-Offsets. 12 = Achtel, 16 = langes
/// Swing-Achtel (2:1 zum kurzen 8), 18 = punktierte Achtel (Shuffle).
const VOKABULAR_GROOVE: &[u32] = &[12, 16, 18];
/// Breakbeat: 16tel mit 32tel-Splits. 6 = 16tel (Grundschritt), 9 = 16tel +
/// 32tel (punktierte 16tel), 3 = 32tel.
const VOKABULAR_BREAKBEAT: &[u32] = &[6, 9, 3];
/// Drum and Bass: schnelle 16tel-Läufe (6, Grundschritt) mit Synkopen; 12 =
/// Achtel, 18 = punktierte Achtel (die 3:2-Synkope).
const VOKABULAR_DRUM_UND_BASS: &[u32] = &[6, 12, 18];

/// Genre zum Namen (Groß-/Kleinschreibung und Leerzeichen sind egal):
/// „Techno“, „Groove“, „Breakbeat“, „Drum and Bass“ (auch „dnb“).
/// Unbekannte Namen ergeben `None`.
pub fn genre(name: &str) -> Option<Genre> {
    let name = name.trim().to_lowercase();
    match name.as_str() {
        "techno" => Some(Genre {
            name: "Techno",
            vokabular: VOKABULAR_TECHNO,
        }),
        "groove" => Some(Genre {
            name: "Groove",
            vokabular: VOKABULAR_GROOVE,
        }),
        "breakbeat" => Some(Genre {
            name: "Breakbeat",
            vokabular: VOKABULAR_BREAKBEAT,
        }),
        "drum and bass" | "drum & bass" | "drum-and-bass" | "drumandbass" | "dnb" => Some(Genre {
            name: "Drum and Bass",
            vokabular: VOKABULAR_DRUM_UND_BASS,
        }),
        _ => None,
    }
}

/// Wie die Abstände zu Schritten werden.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Abstandsmodus {
    /// Nur die Verhältnisse zählen: der kleinste Abstand ist ein Grundschritt
    /// des Genres, die anderen im Verhältnis dazu. Für Fotos ohne Maßstab.
    Relativ,
    /// Physik: Abstand in Metern, Gehgeschwindigkeit in Metern je Sekunde,
    /// Schrittlänge in Metern.
    ///
    /// Die Schrittlänge ist die eines **Menschen** (Vorgabe
    /// [`SCHRITTLANGE_M`]): die Blätter werden beim Gehen abgelegt, ein
    /// Abstand unter einer Schrittlänge ist deshalb trotzdem ein Schritt
    /// („kein halber Schritt“). Sie ist außerdem der Maßstab, mit dem die App
    /// Pixel-Abstände in Meter umrechnet, wenn sie den Weg kennt.
    Absolut {
        /// Gehgeschwindigkeit beim Ablegen der Blätter.
        geschwindigkeit_m_s: f32,
        /// Länge eines menschlichen Schritts.
        schrittlaenge_m: f32,
    },
}

impl Abstandsmodus {
    /// Absoluter Modus mit den Vorgaben: [`GEHTEMPO_M_S`] und
    /// [`SCHRITTLANGE_M`].
    pub fn gehen() -> Self {
        Self::Absolut {
            geschwindigkeit_m_s: GEHTEMPO_M_S,
            schrittlaenge_m: SCHRITTLANGE_M,
        }
    }
}

/// Vokabel in Schritten des gewählten Rasters: `v * raster / 96`, gerundet
/// und mindestens 1.
fn vokabel_skaliert(v: u32, raster_pro_takt: u16) -> u32 {
    let skaliert = (v as u64 * raster_pro_takt as u64 + 48) / 96;
    skaliert.max(1) as u32
}

/// Rohintervall (in Rasterschritten) auf das Genre-Vokabular runden: der
/// nächste Wert der Form `k * v` für eine Vokabel `v` und `k ≥ 1`.
///
/// Das kleinste Ergebnis ist eine Vokabel (ein Abstand von 0 wird also ein
/// Schritt, keine Pause). Genau in der Mitte gewinnt der **kürzere** Wert
/// („halbe Viertel werden abgerundet“, `k = ceil(roh/v − 0,5)`), und bei
/// gleichem Abstand die in der Liste frühere Vokabel — beides macht die
/// Rundung eindeutig und hält den Rhythmus eher kurz als lang.
fn runde_auf_vokabular(roh: f32, vokabeln: &[u32]) -> u32 {
    let mut best: Option<(f32, u32)> = None;
    for v in vokabeln {
        let v = *v as f32;
        let k = (roh / v - 0.5).ceil().max(1.0);
        let wert = v * k;
        let abstand = (wert - roh).abs();
        if best.map(|(bisher, _)| abstand < bisher).unwrap_or(true) {
            best = Some((abstand, wert.round().max(1.0) as u32));
        }
    }
    best.map(|(_, wert)| wert).unwrap_or(1)
}

/// Schrittnummern aus den Abständen entlang des Wegs, mit [`BPM_VORGABE`]
/// (120 BPM) für den absoluten Modus.
///
/// Wie [`schritte_aus_abstaenden_bpm`], nur ohne eigenes Tempo.
pub fn schritte_aus_abstaenden(
    d: &[f32],
    raster_pro_takt: u16,
    modus: Abstandsmodus,
    genre: &Genre,
) -> Vec<u32> {
    schritte_aus_abstaenden_bpm(d, raster_pro_takt, modus, genre, BPM_VORGABE)
}

/// Schrittnummern aus den Abständen entlang des Wegs.
///
/// `d` sind die Abstände **zwischen** aufeinanderfolgenden Blättern (in Metern
/// beim absoluten Modus), `raster_pro_takt` das Raster aus [`rasterwahl`],
/// `bpm` das Tempo für den absoluten Modus (beim relativen egal).
///
/// Das erste Blatt liegt auf Schritt 0; danach wird je Abstand ein Intervall
/// aufsummiert. Jedes Intervall wird auf das Genre-Vokabular gerundet
/// (siehe [`Genre`]) — auch im absoluten Modus, denn das Genre macht erst den
/// Charakter. Die Summe wird im Takt gefaltet (`% raster_pro_takt`), das
/// Ergebnis ist sortiert und ohne Doppelte; es wiederholt sich mit jedem Takt
/// und füllt so die Patternlänge.
///
/// Kaputte Werte (NaN, Unendlich, negativ) fallen einzeln weg; ohne brauchbare
/// Abstände, ohne Raster (0) oder ohne Vokabeln ist das Ergebnis leer. Ein
/// Abstand von 0 ist kein Fehler, sondern ein Schritt. Fehlende
/// Geschwindigkeit/Schrittlänge/Tempo fallen auf ihre Vorgaben zurück
/// ([`GEHTEMPO_M_S`], [`SCHRITTLANGE_M`], [`BPM_VORGABE`]).
pub fn schritte_aus_abstaenden_bpm(
    d: &[f32],
    raster_pro_takt: u16,
    modus: Abstandsmodus,
    genre: &Genre,
    bpm: f32,
) -> Vec<u32> {
    if raster_pro_takt == 0 || genre.vokabular.is_empty() {
        return Vec::new();
    }
    let abstaende: Vec<f32> = d
        .iter()
        .copied()
        .filter(|wert| wert.is_finite() && *wert >= 0.0)
        .collect();
    if abstaende.is_empty() {
        return Vec::new();
    }
    let vokabeln: Vec<u32> = genre
        .vokabular
        .iter()
        .map(|v| vokabel_skaliert(*v, raster_pro_takt))
        .collect();
    let grundschritt = vokabeln[0] as f32;
    let roh: Vec<f32> = match modus {
        Abstandsmodus::Relativ => {
            // Bezug ist der kleinste Abstand > 0; ohne einen (lauter Nullen)
            // ist jeder Abstand genau ein Grundschritt.
            let kleinster = abstaende
                .iter()
                .copied()
                .filter(|wert| *wert > 0.0)
                .fold(f32::INFINITY, f32::min);
            let bezug = if kleinster.is_finite() {
                kleinster
            } else {
                1.0
            };
            abstaende
                .iter()
                .map(|wert| (wert / bezug) * grundschritt)
                .collect()
        }
        Abstandsmodus::Absolut {
            geschwindigkeit_m_s,
            schrittlaenge_m,
        } => {
            let tempo = if geschwindigkeit_m_s.is_finite() && geschwindigkeit_m_s > 0.0 {
                geschwindigkeit_m_s
            } else {
                GEHTEMPO_M_S
            };
            let schritt = if schrittlaenge_m.is_finite() && schrittlaenge_m > 0.0 {
                schrittlaenge_m
            } else {
                SCHRITTLANGE_M
            };
            let bpm = if bpm.is_finite() && bpm > 0.0 {
                bpm
            } else {
                BPM_VORGABE
            };
            abstaende
                .iter()
                .map(|meter| {
                    // Kein halber Schritt: kürzer als eine Schrittlänge gibt es
                    // beim Gehen nicht.
                    let sekunden = meter.max(schritt) / tempo;
                    // Sekunden * BPM / 60 = Schläge, mal Raster/4 = Schritte.
                    sekunden * bpm * raster_pro_takt as f32 / 240.0
                })
                .collect()
        }
    };
    let raster = raster_pro_takt as u64;
    let mut raus: Vec<u32> = Vec::with_capacity(roh.len() + 1);
    raus.push(0);
    let mut position: u64 = 0;
    for interval in roh {
        let schritt = runde_auf_vokabular(interval, &vokabeln) as u64;
        position = (position + schritt) % raster;
        raus.push(position as u32);
    }
    raus.sort_unstable();
    raus.dedup();
    raus
}

/// Bildet die Schritte **eines Takts** (`0..raster`) auf Pattern-Zeilen ab.
///
/// `zeilen_pro_takt` ist die Auflösung des Zielpatterns (16 = eine Zeile je
/// 16tel, 32 = 32tel, 64 = 64tel), `laenge` dessen Länge und `start` die erste
/// Zeile, an die der Takt geschrieben wird (für Wiederholungen und das zweite
/// Pattern). Eine Zeile ist `start + schritt * zeilen_pro_takt / raster`,
/// ganzzahlig abgerundet; Zeilen ab `laenge` fallen weg.
///
/// Mehrere Schritte können auf derselben Zeile landen (feineres Raster als das
/// Pattern): dann gewinnt der **erste** Eintrag der Eingabe — so bleibt die
/// Farbe des Blattes erhalten, das den Schritt zuerst belegt. Das Ergebnis ist
/// nach Zeile sortiert und ohne doppelte Zeilen.
///
/// Ohne Raster oder ohne Zeilen je Takt gibt es keine Abbildung (leeres
/// Ergebnis).
pub fn raster_auf_zeilen(
    takt: &[(u32, u8)],
    raster: u16,
    zeilen_pro_takt: u16,
    laenge: u16,
    start: u16,
) -> Vec<(u16, u8)> {
    if raster == 0 || zeilen_pro_takt == 0 || laenge == 0 {
        return Vec::new();
    }
    // Belegt-Maske: so gewinnt der erste Eintrag der Eingabe, auch wenn die
    // Liste unsortiert ist (ein sortiertes Dedup würde den falschen behalten).
    let mut belegt = vec![false; laenge as usize];
    let mut raus = Vec::with_capacity(takt.len());
    for (schritt, instrument) in takt {
        // u64 statt u16: Schritt * Zeilen kann bei großen Rastern überlaufen.
        let zeile = start as u64 + (*schritt as u64 * zeilen_pro_takt as u64) / raster as u64;
        if zeile >= laenge as u64 {
            continue;
        }
        let zeile = zeile as u16;
        if belegt[zeile as usize] {
            continue;
        }
        belegt[zeile as usize] = true;
        raus.push((zeile, *instrument));
    }
    raus.sort_unstable_by_key(|(zeile, _)| *zeile);
    raus
}

/// Teilt die Abstandsliste in Gruppen von je `pro_gruppe` Abständen; die
/// letzte Gruppe darf kürzer sein. `pro_gruppe == 0` legt alles in **eine**
/// Gruppe, eine leere Eingabe ergibt eine leere Liste.
///
/// Die App verteilt einen langen Weg damit auf mehrere Patterns: jede Gruppe
/// ist ein Takt, und ohne Gruppierung (0) bleibt der ganze Weg ein Takt. Die
/// Reihenfolge der Abstände bleibt erhalten, damit die Wegrichtung stimmt.
pub fn gruppiere_abstaende(abstaende: &[f32], pro_gruppe: usize) -> Vec<Vec<f32>> {
    if abstaende.is_empty() {
        return Vec::new();
    }
    // `chunks(0)` würde panicken; 0 heißt hier „eine Gruppe“.
    let groesse = if pro_gruppe == 0 {
        abstaende.len()
    } else {
        pro_gruppe
    };
    abstaende
        .chunks(groesse)
        .map(|gruppe| gruppe.to_vec())
        .collect()
}

/// Bildet eine Taktfolge auf Gruppen ab: `folge[i]` ist der Index der Gruppe,
/// die Takt `i + 1` spielt. Eine Taktgruppe ist damit z. B. `G1 G2 G1 G3`
/// (`folge = [0, 1, 0, 2]`) — dieselbe Gruppe darf mehrfach vorkommen.
///
/// Warum klemmen statt verwerfen: Die Taktfolge wird in der App gepflegt,
/// während der Nutzer die Gruppengröße ändert; ein Index hinter dem Ende ist
/// dann kein Fehler, sondern meint die letzte vorhandene Gruppe. So bleibt
/// jede Taktposition bedienbar und die Folge behält ihre Länge. Ohne Gruppen
/// oder ohne Folge gibt es keine Takte (leeres Ergebnis).
pub fn taktfolge_gruppen(gruppen: &[Vec<f32>], folge: &[usize]) -> Vec<Vec<f32>> {
    let Some(letzte) = gruppen.len().checked_sub(1) else {
        return Vec::new();
    };
    folge
        .iter()
        .map(|index| gruppen[(*index).min(letzte)].clone())
        .collect()
}

/// Gleichmäßig über die ganze Videodauer verteilte Zeitpunkte für
/// Bild-Stichproben: `anzahl` Abschnitte, je ein Zeitpunkt in der **Mitte**,
/// also `(i + 0.5) / anzahl * dauer_s`.
///
/// Die App prüft beim Video damit nicht mehr nur die ersten Bilder, sondern
/// den ganzen Film (bei einer Fahrt mit Blättern ab Sekunde 5 sähe man sonst
/// nur Weg). Die Mitte des Abschnitts statt seines Anfangs meidet die Ränder:
/// ein Bild bei exakt 0 oder `dauer_s` liegt oft außerhalb des dekodierbaren
/// Bereichs. `anzahl == 0`, eine Dauer ≤ 0 oder ein nicht endlicher Wert
/// ergeben eine leere Liste; der Aufrufer prüft dann nichts.
pub fn stichproben_zeiten(dauer_s: f32, anzahl: usize) -> Vec<f32> {
    if anzahl == 0 || !dauer_s.is_finite() || dauer_s <= 0.0 {
        return Vec::new();
    }
    let n = anzahl as f32;
    (0..anzahl)
        .map(|i| (i as f32 + 0.5) / n * dauer_s)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kurz: zwei Werte mit Nachschärfe vergleichen.
    fn nah(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// Synthetisches Bild: ein heller Weg in der Hintergrundfarbe.
    fn bild(w: usize, h: usize, hg: (u8, u8, u8)) -> Vec<u8> {
        let mut frame = Vec::with_capacity(w * h * 3);
        for _ in 0..w * h {
            frame.push(hg.0);
            frame.push(hg.1);
            frame.push(hg.2);
        }
        frame
    }

    /// Rechteckigen Flecken malen, `x1`/`y1` exklusiv.
    fn fleck(
        f: &mut [u8],
        w: usize,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        farbe: (u8, u8, u8),
    ) {
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y * w + x) * 3;
                f[i] = farbe.0;
                f[i + 1] = farbe.1;
                f[i + 2] = farbe.2;
            }
        }
    }

    /// Ein Blatt für die Sortier- und Farbtests (Fläche egal).
    fn blatt(x: f32, y: f32, r: u8, g: u8, b: u8) -> Blatt {
        Blatt {
            x,
            y,
            flaeche: 9,
            breite: 3,
            hoehe: 3,
            r,
            g,
            b,
        }
    }

    #[test]
    fn vier_flecken_werden_gefunden() {
        // Heller Weg, vier Blätter in verschiedenen Größen und Farben.
        let hg = (205, 200, 190);
        let (w, h) = (40usize, 30usize);
        let mut f = bild(w, h, hg);
        fleck(&mut f, w, 6, 4, 9, 7, (200, 40, 30)); // 3x3 rot
        fleck(&mut f, w, 20, 5, 23, 9, (60, 150, 50)); // 3x4 grün
        fleck(&mut f, w, 10, 15, 14, 19, (230, 200, 50)); // 4x4 gelb
        fleck(&mut f, w, 28, 20, 31, 24, (120, 80, 40)); // 3x4 braun
        let mut bl = blaetter(&f, w as i32, h as i32, hg, 20);
        assert_eq!(bl.len(), 4, "vier Flecken: {bl:?}");
        sortiere_wegrichtung(&mut bl);
        let mittel: Vec<(f32, f32)> = bl.iter().map(|b| (b.x, b.y)).collect();
        assert!(nah(mittel[0].0, 7.0) && nah(mittel[0].1, 5.0), "{mittel:?}");
        assert!(
            nah(mittel[1].0, 21.0) && nah(mittel[1].1, 6.5),
            "{mittel:?}"
        );
        assert!(
            nah(mittel[2].0, 11.5) && nah(mittel[2].1, 16.5),
            "{mittel:?}"
        );
        assert!(
            nah(mittel[3].0, 29.0) && nah(mittel[3].1, 21.5),
            "{mittel:?}"
        );
        // Die Flächen sind die der gemalten Rechtecke.
        let flaechen: Vec<u32> = bl.iter().map(|b| b.flaeche).collect();
        assert_eq!(flaechen, vec![9, 12, 16, 12], "{flaechen:?}");
    }

    #[test]
    fn hintergrund_zaehlt_nicht() {
        let hg = (205, 200, 190);
        let (w, h) = (20usize, 10usize);
        let mut f = bild(w, h, hg);
        // Ein Punkt knapp innerhalb der Toleranz ist noch Weg.
        fleck(&mut f, w, 0, 0, 1, 1, (210, 200, 190));
        assert!(
            blaetter(&f, w as i32, h as i32, hg, 8).is_empty(),
            "Unterschied 5 bei Toleranz 8: noch Hintergrund"
        );
        // Mit engerer Toleranz zählt er, ist aber zu klein (MIN_FLAECHE).
        assert!(
            blaetter(&f, w as i32, h as i32, hg, 2).is_empty(),
            "ein einzelner Punkt ist kein Blatt"
        );
        // Erst ein Flecken über der Mindestfläche wird eins.
        fleck(&mut f, w, 5, 5, 7, 7, (60, 150, 50));
        let bl = blaetter(&f, w as i32, h as i32, hg, 8);
        assert_eq!(bl.len(), 1, "{bl:?}");
        assert_eq!(bl[0].flaeche, 4);
    }

    #[test]
    fn stoerpixel_zaehlen_nicht() {
        let hg = (205, 200, 190);
        let (w, h) = (16usize, 8usize);
        let mut f = bild(w, h, hg);
        // Einzelpixel, Paar und Dreierlinie: alle unter MIN_FLAECHE.
        fleck(&mut f, w, 2, 2, 3, 3, (200, 40, 30));
        fleck(&mut f, w, 5, 2, 7, 3, (60, 150, 50));
        fleck(&mut f, w, 9, 2, 12, 3, (120, 80, 40));
        // Ein echter Flecken (2x2 = 4) bleibt.
        fleck(&mut f, w, 13, 4, 15, 6, (230, 200, 50));
        let bl = blaetter(&f, w as i32, h as i32, hg, 20);
        assert_eq!(bl.len(), 1, "nur der 2x2-Flecken: {bl:?}");
        assert_eq!(bl[0].flaeche, 4);
        assert_eq!((bl[0].r, bl[0].g, bl[0].b), (230, 200, 50));
    }

    #[test]
    fn flaeche_und_farbe() {
        let hg = (205, 200, 190);
        let (w, h) = (12usize, 8usize);
        let mut f = bild(w, h, hg);
        // 3x2 = 6 Punkte, eine Farbe: Fläche, Mittelpunkt und Farbe exakt.
        fleck(&mut f, w, 2, 3, 5, 5, (100, 110, 120));
        let bl = blaetter(&f, w as i32, h as i32, hg, 20);
        assert_eq!(bl.len(), 1, "{bl:?}");
        assert_eq!(bl[0].flaeche, 6);
        assert_eq!((bl[0].r, bl[0].g, bl[0].b), (100, 110, 120));
        assert!(nah(bl[0].x, 3.0), "Spalten 2..5 -> Mitte 3: {}", bl[0].x);
        assert!(nah(bl[0].y, 3.5), "Zeilen 3..5 -> Mitte 3,5: {}", bl[0].y);
        // Zwei Farben in einem Flecken: die Mittelfarbe ist der Mittelwert.
        let mut f2 = bild(w, h, hg);
        fleck(&mut f2, w, 2, 2, 4, 6, (200, 0, 0)); // 2x4 = 8 links
        fleck(&mut f2, w, 4, 2, 6, 6, (0, 0, 200)); // 2x4 = 8 rechts
        let bl2 = blaetter(&f2, w as i32, h as i32, hg, 20);
        assert_eq!(bl2.len(), 1, "berühren sich: {bl2:?}");
        assert_eq!(bl2[0].flaeche, 16);
        assert_eq!(
            (bl2[0].r, bl2[0].g, bl2[0].b),
            (100, 0, 100),
            "Mittelwert beider Hälften"
        );
    }

    #[test]
    fn diagonale_beruehrung_gehoert_zusammen() {
        // 8er-Nachbarschaft: eine Diagonale ist ein Flecken, kein vierfacher.
        let hg = (205, 200, 190);
        let (w, h) = (10usize, 10usize);
        let mut f = bild(w, h, hg);
        for i in 0..4 {
            fleck(&mut f, w, i, i, i + 1, i + 1, (200, 40, 30));
        }
        let bl = blaetter(&f, w as i32, h as i32, hg, 20);
        assert_eq!(bl.len(), 1, "eine Kette: {bl:?}");
        assert_eq!(bl[0].flaeche, 4);
        assert!(nah(bl[0].x, 1.5) && nah(bl[0].y, 1.5), "{:?}", bl[0]);
    }

    #[test]
    fn ein_grosser_flecken_bleibt_einer() {
        let hg = (205, 200, 190);
        let (w, h) = (20usize, 20usize);
        let mut f = bild(w, h, hg);
        fleck(&mut f, w, 5, 5, 15, 15, (200, 40, 30)); // 10x10 = 100
        let bl = blaetter(&f, w as i32, h as i32, hg, 20);
        assert_eq!(bl.len(), 1, "ein Flecken, nicht hundert: {bl:?}");
        assert_eq!(bl[0].flaeche, 100);
        assert!(nah(bl[0].x, 9.5) && nah(bl[0].y, 9.5), "{:?}", bl[0]);
    }

    #[test]
    fn kaputte_bilder_sind_leer() {
        let hg = (205, 200, 190);
        assert!(blaetter(&[], 0, 0, hg, 10).is_empty(), "kein Bild");
        assert!(blaetter(&[], 10, 10, hg, 10).is_empty(), "zu kurz");
        assert!(
            blaetter(&vec![0u8; 299], 10, 10, hg, 10).is_empty(),
            "10x10 braucht 300 Bytes"
        );
        assert!(blaetter(&[0u8; 300], -1, 10, hg, 10).is_empty(), "Breite");
        assert!(blaetter(&[0u8; 300], 10, 0, hg, 10).is_empty(), "Höhe");
        // Genau passend: alles Hintergrund, kein Blatt.
        assert!(blaetter(&[200u8; 300], 10, 10, (200, 200, 200), 10).is_empty());
    }

    #[test]
    fn sortierung_nach_wegrichtung() {
        let mut bl = vec![
            blatt(30.0, 10.0, 0, 0, 0),
            blatt(5.0, 20.0, 0, 0, 0),
            blatt(10.0, 10.0, 0, 0, 0),
        ];
        sortiere_wegrichtung(&mut bl);
        let reihenfolge: Vec<(f32, f32)> = bl.iter().map(|b| (b.x, b.y)).collect();
        assert_eq!(
            reihenfolge,
            vec![(10.0, 10.0), (30.0, 10.0), (5.0, 20.0)],
            "erst y, dann x"
        );
        // Schon sortiert bleibt sortiert (und leer bleibt leer).
        sortiere_wegrichtung(&mut bl);
        assert_eq!(
            bl.iter().map(|b| (b.x, b.y)).collect::<Vec<_>>(),
            reihenfolge
        );
        sortiere_wegrichtung(&mut []);
    }

    #[test]
    fn farbklassen() {
        // Die vier Grundfarben eines Herbstlaubs.
        assert_eq!(farbklasse(&blatt(0.0, 0.0, 200, 40, 30)), FARBE_ROT);
        assert_eq!(farbklasse(&blatt(0.0, 0.0, 230, 200, 50)), FARBE_GELB);
        assert_eq!(farbklasse(&blatt(0.0, 0.0, 120, 80, 40)), FARBE_BRAUN);
        assert_eq!(farbklasse(&blatt(0.0, 0.0, 60, 150, 50)), FARBE_GRUEN);
        // Dunkel bleibt seine Farbe: Rot ist auch dunkel rot, Grün bleibt
        // grün; dunkles Orange/Gelb ist braun (Herbstlaub).
        assert_eq!(farbklasse(&blatt(0.0, 0.0, 120, 30, 25)), FARBE_ROT);
        assert_eq!(farbklasse(&blatt(0.0, 0.0, 30, 70, 25)), FARBE_GRUEN);
        assert_eq!(
            farbklasse(&blatt(0.0, 0.0, 100, 50, 20)),
            FARBE_BRAUN,
            "dunkles Orange"
        );
        // Grau/Beige hat keinen Farbton und zählt zu braun.
        assert_eq!(farbklasse(&blatt(0.0, 0.0, 180, 175, 170)), FARBE_BRAUN);
        assert_eq!(farbklasse_rgb(0, 0, 0), FARBE_BRAUN, "Schwarz");
        // Namen für die Anzeige.
        assert_eq!(farbklasse_name(FARBE_ROT), "rot");
        assert_eq!(farbklasse_name(FARBE_GELB), "gelb");
        assert_eq!(farbklasse_name(FARBE_BRAUN), "braun");
        assert_eq!(farbklasse_name(FARBE_GRUEN), "grün");
        assert_eq!(farbklasse_name(9), "?");
    }

    /// Die Mischfarben-Klassifikation: reine Töne bleiben ihre Klasse,
    /// Grenzfälle entscheidet der Mischwert (nicht die harte Schwelle).
    #[test]
    fn farbklassen_gemischt() {
        // Reine Töne: dieselben Klassen wie exakt — sonst wäre der Umschalter
        // eine Überraschung.
        assert_eq!(farbklasse_mix(200, 40, 30), FARBE_ROT);
        assert_eq!(farbklasse_mix(230, 200, 50), FARBE_GELB);
        assert_eq!(farbklasse_mix(60, 150, 50), FARBE_GRUEN);
        assert_eq!(farbklasse_mix(120, 80, 40), FARBE_BRAUN, "Herbstbraun");
        // Grau/Beige, sehr dunkel und sehr hell haben keinen Farbton: braun.
        assert_eq!(farbklasse_mix(180, 175, 170), FARBE_BRAUN, "Grau");
        assert_eq!(farbklasse_mix(10, 10, 10), FARBE_BRAUN, "sehr dunkel");
        assert_eq!(farbklasse_mix(255, 255, 255), FARBE_BRAUN, "sehr hell");
        // Der Grenzfall Rot↔Gelb: (220, 140, 40) ist ein Herbstorange und
        // liegt dem Gelb näher — beide Verfahren sagen gelb.
        assert_eq!(farbklasse_rgb(220, 140, 40), FARBE_GELB, "exakt: gelb");
        assert_eq!(
            farbklasse_mix(220, 140, 40),
            FARBE_GELB,
            "gemischt: gelb (näher an Gelb als an Rot)"
        );
        // Und hier der Unterschied: 21° Farbton liegt exakt schon im Gelb,
        // der Mischwert kippt erst bei 25° (gleicher Abstand zu Rot und Gelb).
        assert_eq!(farbklasse_rgb(220, 110, 50), FARBE_GELB, "exakt: gelb");
        assert_eq!(
            farbklasse_mix(220, 110, 50),
            FARBE_ROT,
            "gemischt: noch rot"
        );
        // Gleiche Eingabe, gleiche Klasse (keine Streuung durch Zufall).
        for _ in 0..5 {
            assert_eq!(farbklasse_mix(220, 110, 50), FARBE_ROT);
            assert_eq!(farbklasse_mix(120, 80, 40), FARBE_BRAUN);
        }
        // Auch über ein Blatt.
        assert_eq!(
            farbklasse_blatt_mix(&blatt(0.0, 0.0, 230, 200, 50)),
            FARBE_GELB
        );
    }

    /// Die Mischwerte selbst: vier Gewichte, die sich zu 1 summieren und den
    /// Grenzfall zwischen zwei Klassen aufteilen.
    #[test]
    fn farbmischwerte_teilen_den_grenzfall() {
        let rein = farbmischwerte(200, 40, 30);
        assert!(nah(rein.iter().sum::<f32>(), 1.0), "Summe 1: {rein:?}");
        assert!(
            rein[FARBE_ROT as usize] > rein[FARBE_GELB as usize],
            "reines Rot liegt vorn bei rot: {rein:?}"
        );
        assert!(
            rein[FARBE_ROT as usize] > 0.4,
            "… und deutlich: {rein:?}"
        );
        let grenze = farbmischwerte(220, 140, 40);
        assert!(nah(grenze.iter().sum::<f32>(), 1.0), "Summe 1: {grenze:?}");
        assert!(
            grenze[FARBE_GELB as usize] > grenze[FARBE_ROT as usize],
            "Orange teilt sich zwischen gelb und rot: {grenze:?}"
        );
        assert!(
            grenze[FARBE_ROT as usize] > 0.3,
            "… mit einem ordentlichen Rotanteil: {grenze:?}"
        );
        // Grau hat den größten Braunanteil, keine gesättigte Farbe.
        let grau = farbmischwerte(180, 175, 170);
        assert!(nah(grau.iter().sum::<f32>(), 1.0), "Summe 1: {grau:?}");
        assert!(
            grau[FARBE_BRAUN as usize] > grau[FARBE_ROT as usize] + 0.1,
            "Grau ist braun: {grau:?}"
        );
    }

    /// Ein Bild mit vier verschieden großen Flecken für die Filtertests:
    /// 2×2 rot, 3×3 grau, 4×4 gelb, 6×6 grün.
    fn filterbild() -> (Vec<u8>, usize, usize, (u8, u8, u8)) {
        let hg = (205, 200, 190);
        let (w, h) = (40usize, 30usize);
        let mut f = bild(w, h, hg);
        fleck(&mut f, w, 2, 2, 4, 4, (200, 40, 30)); // 4 Punkte, Sättigung hoch
        fleck(&mut f, w, 8, 2, 11, 5, (180, 175, 170)); // 9 Punkte, grau
        fleck(&mut f, w, 16, 2, 20, 6, (230, 200, 50)); // 16 Punkte, gelb
        fleck(&mut f, w, 25, 2, 31, 8, (60, 150, 50)); // 36 Punkte, grün
        (f, w, h, hg)
    }

    /// Die Filter sind Vorgaben-kompatibel: ohne Einschränkung dasselbe wie
    /// `blaetter`, mit Fläche, Sättigung und Helligkeit lässt sich ausdünnen.
    #[test]
    fn filter_waehlen_blaetter_aus() {
        let (f, w, h, hg) = filterbild();
        let alle = blaetter(&f, w as i32, h as i32, hg, 20);
        assert_eq!(alle.len(), 4, "vier Flecken: {alle:?}");
        // Die Vorgabe ist neutral: kein Filter, keine Regression.
        let vorgabe = Blattfilter::default();
        assert_eq!(
            blaetter_mit_filter(&f, w as i32, h as i32, hg, 20, &vorgabe).len(),
            4,
            "Vorgabe filtert nichts weg"
        );
        // Mindestfläche: nur die großen Flecken bleiben.
        let gross = Blattfilter {
            min_flaeche: 16,
            ..Blattfilter::default()
        };
        let bl = blaetter_mit_filter(&f, w as i32, h as i32, hg, 20, &gross);
        assert_eq!(bl.len(), 2, "16 und 36 Punkte: {bl:?}");
        assert!(bl.iter().all(|b| b.flaeche >= 16), "{bl:?}");
        // Höchstfläche: nur die kleinen bleiben (4 und 9).
        let klein = Blattfilter {
            max_flaeche: 9,
            ..Blattfilter::default()
        };
        let bl = blaetter_mit_filter(&f, w as i32, h as i32, hg, 20, &klein);
        assert_eq!(bl.len(), 2, "4 und 9 Punkte: {bl:?}");
        assert!(bl.iter().all(|b| b.flaeche <= 9), "{bl:?}");
        // Mindest-Sättigung: der graue Flecken fällt weg.
        let bunt = Blattfilter {
            min_saettigung: 0.5,
            ..Blattfilter::default()
        };
        let bl = blaetter_mit_filter(&f, w as i32, h as i32, hg, 20, &bunt);
        assert_eq!(bl.len(), 3, "ohne Grau: {bl:?}");
        assert!(
            !bl.iter().any(|b| (b.r, b.g, b.b) == (180, 175, 170)),
            "Grau ist raus: {bl:?}"
        );
        // Helligkeitsbereich: nur mittelhelle Blätter (Gelb und Grün sind
        // heller als 100, Rot ist es auch — Grau liegt dazwischen).
        let mittel = Blattfilter {
            min_helligkeit: 150,
            max_helligkeit: 230,
            ..Blattfilter::default()
        };
        let bl = blaetter_mit_filter(&f, w as i32, h as i32, hg, 20, &mittel);
        assert!(
            bl.iter().all(|b| (150..=230).contains(&b.r.max(b.g).max(b.b))),
            "nur im Bereich: {bl:?}"
        );
        // Verdrehter Helligkeitsbereich wird getauscht statt alles zu
        // verwerfen — ein Regler darf nicht in eine leere Anzeige führen.
        let verdreht = Blattfilter {
            min_helligkeit: 230,
            max_helligkeit: 150,
            ..Blattfilter::default()
        };
        let gleich = Blattfilter {
            min_helligkeit: 150,
            max_helligkeit: 230,
            ..Blattfilter::default()
        };
        assert_eq!(
            blaetter_mit_filter(&f, w as i32, h as i32, hg, 20, &verdreht).len(),
            blaetter_mit_filter(&f, w as i32, h as i32, hg, 20, &gleich).len()
        );
        // Sättigung wird auf 0…1 geklemmt; > 1 filtert alles weg, < 0 nichts.
        let zu_bunt = Blattfilter {
            min_saettigung: 7.0,
            ..Blattfilter::default()
        };
        assert!(
            blaetter_mit_filter(&f, w as i32, h as i32, hg, 20, &zu_bunt).is_empty(),
            "Sättigung 1 ist das Maximum"
        );
    }

    /// Jedes Blatt kennt seine Ausdehnung (Bounding-Box) — daraus kommt die
    /// Form, nicht aus der bloßen Fläche.
    #[test]
    fn blaetter_kennen_ihre_ausdehnung() {
        let hg = (205, 200, 190);
        let (w, h) = (20usize, 20usize);
        let mut f = bild(w, h, hg);
        fleck(&mut f, w, 3, 2, 6, 6, (200, 40, 30)); // 3 breit, 4 hoch
        let bl = blaetter(&f, w as i32, h as i32, hg, 20);
        assert_eq!(bl.len(), 1, "{bl:?}");
        assert_eq!((bl[0].breite, bl[0].hoehe), (3, 4), "Bounding-Box: {bl:?}");
        assert_eq!(bl[0].flaeche, 12);
    }

    /// Formklasse über Seitenverhältnis und Füllgrad: rund, länglich,
    /// gezackt/ausgefranst.
    #[test]
    fn formklassen() {
        let form = |breite: u32, hoehe: u32, flaeche: u32| Blatt {
            x: 0.0,
            y: 0.0,
            flaeche,
            breite,
            hoehe,
            r: 200,
            g: 40,
            b: 30,
        };
        // Quadratisch und gut gefüllt ist rund (Kreis ≈ 78 % der Box).
        assert_eq!(formklasse(&form(10, 10, 100)), FORM_RUND);
        assert_eq!(formklasse(&form(10, 10, 78)), FORM_RUND);
        // Deutlich breiter oder höher ist länglich, auch voll gefüllt.
        assert_eq!(formklasse(&form(20, 4, 80)), FORM_LAENGLICH);
        assert_eq!(formklasse(&form(4, 20, 80)), FORM_LAENGLICH);
        // Ein Blatt mit Zacken füllt seine Box schlecht: gezackt.
        assert_eq!(formklasse(&form(10, 10, 30)), FORM_GEZACKT);
        // Genau an den Grenzen bleibt es eindeutig.
        assert_eq!(formklasse(&form(20, 10, 200)), FORM_LAENGLICH, "2:1");
        assert_eq!(formklasse(&form(10, 10, 55)), FORM_GEZACKT, "55 %");
        // Ohne Fläche (oder ohne Box) gibt es keine Form: rund als neutraler
        // Wert, damit die Anzeige nichts Falsches behauptet.
        assert_eq!(formklasse(&form(10, 10, 0)), FORM_RUND);
        assert_eq!(formklasse(&form(0, 0, 0)), FORM_RUND);
        // Namen für die Anzeige.
        assert_eq!(formklasse_name(FORM_RUND), "rund");
        assert_eq!(formklasse_name(FORM_LAENGLICH), "länglich");
        assert_eq!(formklasse_name(FORM_GEZACKT), "gezackt");
        assert_eq!(formklasse_name(9), "?");
    }

    /// Die Gruppen-Beschreibung nennt Schritte, Farbe (mit Modus) und Form —
    /// daraus liest der Nutzer die „Diversitäten“ der Taktabstände ab.
    #[test]
    fn gruppenbeschreibung_nennt_schritte_farbe_form() {
        let rund = |r: u8, g: u8, b: u8| Blatt {
            x: 0.0,
            y: 0.0,
            flaeche: 78,
            breite: 10,
            hoehe: 10,
            r,
            g,
            b,
        };
        let drei_rot = vec![
            rund(200, 40, 30),
            rund(190, 35, 25),
            rund(210, 45, 35),
        ];
        assert_eq!(
            beschreibe_gruppe(1, 4, &drei_rot, true),
            "G1: 4 Schritte · rot-gemischt · rund (3 Blätter)"
        );
        assert_eq!(
            beschreibe_gruppe(1, 4, &drei_rot, false),
            "G1: 4 Schritte · rot-exakt · rund (3 Blätter)"
        );
        // Zwei Farben in einer Gruppe: beide stehen da, die häufigere zuerst.
        let gemischt = vec![
            rund(200, 40, 30),
            rund(190, 35, 25),
            rund(230, 200, 50),
        ];
        assert_eq!(
            beschreibe_gruppe(2, 3, &gemischt, true),
            "G2: 3 Schritte · rot/gelb-gemischt · rund (3 Blätter)"
        );
        // Ohne Blätter bleibt die Zeile ehrlich.
        assert_eq!(
            beschreibe_gruppe(3, 2, &[], true),
            "G3: 2 Schritte · ohne Blätter"
        );
    }

    /// Untergruppen zählen die Blätter einer Gruppe nach Farb- und Form-Mix.
    #[test]
    fn untergruppen_zaehlen_nach_farbe_und_form() {
        let blatt_mit = |breite: u32, hoehe: u32, flaeche: u32, r: u8, g: u8, b: u8| Blatt {
            x: 0.0,
            y: 0.0,
            flaeche,
            breite,
            hoehe,
            r,
            g,
            b,
        };
        let blaetter = vec![
            blatt_mit(10, 10, 78, 200, 40, 30), // rot, rund
            blatt_mit(10, 10, 78, 190, 35, 25), // rot, rund
            blatt_mit(20, 4, 80, 230, 200, 50), // gelb, länglich
        ];
        assert_eq!(
            beschreibe_untergruppen(1, &blaetter, true),
            "G1a: rot/gemischt · rund, 2 · G1b: gelb/gemischt · länglich, 1"
        );
        // Exakt-Modus wird genauso ausgewiesen; ohne Blätter leer.
        assert!(beschreibe_untergruppen(1, &blaetter, false).contains("rot/exakt"));
        assert!(beschreibe_untergruppen(1, &[], true).is_empty());
    }

    #[test]
    fn relativ_kleinster_abstand_ist_ein_schritt() {
        let techno = genre("Techno").expect("Techno");
        // Abstände 2, 4 und 8 Meter: der kleinste ist ein Grundschritt
        // (Viertel = 24 bei Raster 96), die anderen 2 und 4 davon.
        let s = schritte_aus_abstaenden(&[2.0, 4.0, 8.0], 96, Abstandsmodus::Relativ, &techno);
        assert_eq!(s, vec![0, 24, 72], "0; 24; 24+48; 24+48+96 % 96");
        // Gleiche Abstände ergeben gleiche Schritte.
        let s = schritte_aus_abstaenden(&[1.0, 1.0, 1.0], 96, Abstandsmodus::Relativ, &techno);
        assert_eq!(s, vec![0, 24, 48, 72], "viermal eine Viertel");
        // Der Puls des Genres ist der Grundschritt: Groove = Achtel (12).
        let groove = genre("Groove").expect("Groove");
        let s = schritte_aus_abstaenden(&[1.0, 1.0, 1.0], 96, Abstandsmodus::Relativ, &groove);
        assert_eq!(s, vec![0, 12, 24, 36]);
    }

    #[test]
    fn absolut_rechnet_mit_gehen() {
        let techno = genre("Techno").expect("Techno");
        let modus = Abstandsmodus::Absolut {
            geschwindigkeit_m_s: 1.2,
            schrittlaenge_m: 0.75,
        };
        // 1,2 m bei 1,2 m/s = 1 s. Bei 120 BPM ist eine Viertel 0,5 s, bei
        // 96 Schritten pro Takt also 24 Schritte: 1 s = 48 Schritte.
        let s = schritte_aus_abstaenden(&[1.2, 1.2], 96, modus, &techno);
        assert_eq!(s, vec![0, 48], "1 s = zwei Viertel");
        // Kein halber Schritt: 10 cm sind trotzdem eine Schrittlänge (0,75 m
        // = 0,625 s = 30 Schritte), Techno rundet auf die Viertel (24).
        let s = schritte_aus_abstaenden(&[0.1], 96, modus, &techno);
        assert_eq!(s, vec![0, 24], "unter einer Schrittlänge = ein Schritt");
        // Das Tempo kommt vom Aufrufer: bei 60 BPM ist 1 s nur eine Viertel.
        let s = schritte_aus_abstaenden_bpm(&[1.2], 96, modus, &techno, 60.0);
        assert_eq!(s, vec![0, 24], "halbes Tempo, halbe Schritte");
        // Und die Vorgabe ist 120 BPM.
        assert_eq!(
            schritte_aus_abstaenden(&[1.2], 96, modus, &techno),
            schritte_aus_abstaenden_bpm(&[1.2], 96, modus, &techno, BPM_VORGABE)
        );
        assert_eq!(
            Abstandsmodus::gehen(),
            modus,
            "die Vorgaben sind 1,2 / 0,75"
        );
    }

    #[test]
    fn genre_rundet_auf_vokabular() {
        let groove = genre("Groove").expect("Groove");
        // Verhältnisse 1 / 2,4 / 3,9 mit Grundschritt 12: roh 12 / 28,8 /
        // 46,8. Das Vokabular (12, 16, 18) rundet auf 12 / 32 / 48.
        let s = schritte_aus_abstaenden(&[1.0, 2.4, 3.9], 96, Abstandsmodus::Relativ, &groove);
        assert_eq!(s, vec![0, 12, 44, 92], "12; +32; +48");
        // Breakbeat: 16tel-Grundschritt (6), 1,5-faches wird der
        // 16tel+32tel-Split (9).
        let breakbeat = genre("Breakbeat").expect("Breakbeat");
        let s = schritte_aus_abstaenden(&[1.0, 1.5], 96, Abstandsmodus::Relativ, &breakbeat);
        assert_eq!(s, vec![0, 6, 15]);
        // Drum and Bass: die Synkope 3:2 landet auf der punktierten Achtel.
        let dnb = genre("Drum and Bass").expect("Drum and Bass");
        let s = schritte_aus_abstaenden(&[1.0, 3.0], 96, Abstandsmodus::Relativ, &dnb);
        assert_eq!(s, vec![0, 6, 24], "16tel; 16tel + punktierte Achtel");
        // Techno kennt nur die Viertel: 3,9 Viertel werden vier (ein ganzer
        // Takt), die Position fällt damit auf die Viertel zurück.
        let techno = genre("Techno").expect("Techno");
        let s = schritte_aus_abstaenden(&[1.0, 3.9], 96, Abstandsmodus::Relativ, &techno);
        assert_eq!(s, vec![0, 24], "24; 24 + 96 % 96");
    }

    #[test]
    fn raster_skaliert() {
        assert_eq!(rasterwahl(), [96, 128, 192, 256]);
        let techno = genre("Techno").expect("Techno");
        // Derselbe Abstandssatz liegt in jedem Raster musikalisch gleich:
        // der kleinste Abstand ist immer eine Viertel (Raster/4).
        for raster in rasterwahl() {
            let viertel = raster as u32 / 4;
            let s = schritte_aus_abstaenden(&[2.0, 4.0], raster, Abstandsmodus::Relativ, &techno);
            assert_eq!(s, vec![0, viertel, 3 * viertel], "Raster {raster}");
        }
        // Breakbeat: 16tel = Raster/16, der 16tel+32tel-Split = 3*Raster/32.
        let breakbeat = genre("Breakbeat").expect("Breakbeat");
        for raster in rasterwahl() {
            let sechzehntel = raster as u32 / 16;
            let split = 3 * raster as u32 / 32;
            let s =
                schritte_aus_abstaenden(&[1.0, 1.5], raster, Abstandsmodus::Relativ, &breakbeat);
            assert_eq!(
                s,
                vec![0, sechzehntel, sechzehntel + split],
                "Raster {raster}"
            );
        }
    }

    #[test]
    fn leere_und_kaputte_werte() {
        let techno = genre("Techno").expect("Techno");
        // Ohne Abstände kein Rhythmus.
        assert!(schritte_aus_abstaenden(&[], 96, Abstandsmodus::Relativ, &techno).is_empty());
        // Nichts Brauchbares in der Liste.
        assert!(schritte_aus_abstaenden(
            &[f32::NAN, f32::INFINITY, -1.0],
            96,
            Abstandsmodus::Relativ,
            &techno
        )
        .is_empty());
        // Ohne Raster gibt es keine Schrittnummern.
        assert!(schritte_aus_abstaenden(&[1.0], 0, Abstandsmodus::Relativ, &techno).is_empty());
        // Ein Genre ohne Vokabeln ergibt nichts.
        let ohne = Genre {
            name: "leer",
            vokabular: &[],
        };
        assert!(schritte_aus_abstaenden(&[1.0], 96, Abstandsmodus::Relativ, &ohne).is_empty());
        // Kaputte Einzelwerte fallen weg, der Rest bleibt.
        let s = schritte_aus_abstaenden(&[1.0, f32::NAN, 2.0], 96, Abstandsmodus::Relativ, &techno);
        assert_eq!(s, vec![0, 24, 72]);
        // Ein Abstand von 0 ist ein Schritt, keine Pause.
        let s = schritte_aus_abstaenden(&[0.0, 1.0], 96, Abstandsmodus::Relativ, &techno);
        assert_eq!(s, vec![0, 24, 48]);
        // Kaputte Modus-Werte fallen auf die Vorgaben zurück.
        let kaputt = Abstandsmodus::Absolut {
            geschwindigkeit_m_s: 0.0,
            schrittlaenge_m: -1.0,
        };
        assert_eq!(
            schritte_aus_abstaenden(&[1.2], 96, kaputt, &techno),
            vec![0, 48]
        );
        assert_eq!(
            schritte_aus_abstaenden_bpm(&[1.2], 96, kaputt, &techno, 0.0),
            vec![0, 48],
            "auch ein kaputtes Tempo fällt auf die Vorgabe zurück"
        );
        // Unbekannte Genres gibt es nicht.
        assert!(genre("").is_none());
        assert!(genre("Walzer").is_none());
    }

    #[test]
    fn genre_namen_und_vokabeln() {
        let erwartet = [
            ("Techno", 24u32),
            ("Groove", 12),
            ("Breakbeat", 6),
            ("Drum and Bass", 6),
        ];
        for (name, grundschritt) in erwartet {
            let g = genre(name).unwrap_or_else(|| panic!("{name} fehlt"));
            assert_eq!(g.name, name);
            assert_eq!(g.vokabular[0], grundschritt, "Grundschritt von {name}");
            assert!(g.vokabular.iter().all(|v| *v > 0), "Vokabeln positiv");
            assert!(g.vokabular.len() <= 4, "übersichtlich");
        }
        // Kleinschreibung und Kurzform.
        assert_eq!(genre(" techno ").map(|g| g.name), Some("Techno"));
        assert_eq!(genre("GROOVE").map(|g| g.name), Some("Groove"));
        assert_eq!(genre("breakbeat").map(|g| g.name), Some("Breakbeat"));
        assert_eq!(genre("dnb").map(|g| g.name), Some("Drum and Bass"));
    }

    #[test]
    fn schritte_liegen_im_takt() {
        let techno = genre("Techno").expect("Techno");
        // Ein langer Weg: die Schrittnummern wiederholen sich, bleiben aber
        // immer im Takt.
        let abstaende: Vec<f32> = (0..40).map(|i| 1.2 + (i % 3) as f32 * 0.3).collect();
        let s = schritte_aus_abstaenden(&abstaende, 96, Abstandsmodus::gehen(), &techno);
        assert!(!s.is_empty());
        assert!(s.iter().all(|schritt| *schritt < 96), "{s:?}");
        assert!(s.windows(2).all(|w| w[0] < w[1]), "sortiert ohne Doppelte");
        assert_eq!(s.first(), Some(&0), "das erste Blatt beginnt den Takt");
        assert!(
            s.len() < abstaende.len(),
            "weniger Schrittnummern als Blätter: die Schritte wiederholen sich"
        );
        assert_eq!(s, vec![0, 24, 48, 72], "Techno: die vier Viertel");
    }

    #[test]
    fn fuenf_abstaende_werden_ein_vier_auf_dem_boden() {
        // Das Rechenbeispiel aus dem Modulkopf: 1,2 / 1,8 / 1,2 / 2,4 / 1,2 m
        // bei 1,2 m/s, 120 BPM, Raster 96, Techno.
        let techno = genre("Techno").expect("Techno");
        let s = schritte_aus_abstaenden(
            &[1.2, 1.8, 1.2, 2.4, 1.2],
            96,
            Abstandsmodus::gehen(),
            &techno,
        );
        assert_eq!(s, vec![0, 24, 48, 72]);
    }

    #[test]
    fn raster_128_auf_16_zeilen() {
        // Ein Takt mit 128 Rasterzellen belegt bei 1/16 genau 16 Zeilen:
        // 0→0, Viertel (32) → 4, Halbe (64) → 8, letzte Zelle → 15.
        let takt = [(0, 0), (24, 1), (32, 2), (64, 3), (127, 0)];
        assert_eq!(
            raster_auf_zeilen(&takt, 128, 16, 16, 0),
            vec![(0, 0), (3, 1), (4, 2), (8, 3), (15, 0)]
        );
    }

    #[test]
    fn raster_96_auf_32_zeilen() {
        // 96er-Raster auf ein 32er-Pattern: die Achtel (12) wird Zeile 4,
        // die Viertel (24) Zeile 8, die letzte Zelle (95) Zeile 31.
        let takt = [(0, 0), (12, 1), (24, 2), (48, 3), (95, 0)];
        assert_eq!(
            raster_auf_zeilen(&takt, 96, 32, 32, 0),
            vec![(0, 0), (4, 1), (8, 2), (16, 3), (31, 0)]
        );
    }

    #[test]
    fn raster_256_auf_64_zeilen() {
        // 256er-Raster auf ein 64er-Pattern: Viertel 64 → 16, letzte Zelle
        // 255 → 63 (abgerundet, nicht 64).
        let takt = [(0, 0), (64, 1), (128, 2), (255, 3)];
        assert_eq!(
            raster_auf_zeilen(&takt, 256, 64, 64, 0),
            vec![(0, 0), (16, 1), (32, 2), (63, 3)]
        );
    }

    #[test]
    fn start_versatz_und_laenge_schneiden_ab() {
        // `start` ist die erste Zeile im Zielpattern (Wiederholung des Takts);
        // was hinter `laenge` liegt, fällt weg.
        let takt = [(0, 0), (31, 1), (32, 2), (63, 3), (64, 0)];
        assert_eq!(
            raster_auf_zeilen(&takt, 128, 16, 12, 8),
            vec![(8, 0), (11, 1)],
            "0→8, 31→11; ab Zeile 12 (32→12) wird verworfen"
        );
        // Start schon hinter dem Ende: nichts.
        assert!(raster_auf_zeilen(&takt, 128, 16, 16, 20).is_empty());
    }

    #[test]
    fn gleiche_zeile_erste_note_gewinnt() {
        // 128er-Raster, 16 Zeilen: 0…7 liegen alle auf Zeile 0. Der erste
        // Eintrag in Eingabereihenfolge gewinnt, auch wenn die Liste
        // unsortiert ist; die Ausgabe ist nach Zeile sortiert.
        let takt = [(7, 3), (0, 1), (8, 2), (15, 0)];
        assert_eq!(
            raster_auf_zeilen(&takt, 128, 16, 32, 0),
            vec![(0, 3), (1, 2)],
            "7 und 0 → Zeile 0 (erster: Instrument 3); 8 und 15 → Zeile 1"
        );
        // Für Zeile 1 gewinnt 8 (Instrument 2), 15 wird verworfen.
    }

    #[test]
    fn raster_auf_zeilen_leer_und_null() {
        assert!(raster_auf_zeilen(&[], 128, 16, 128, 0).is_empty());
        let takt = [(0, 0), (64, 1)];
        assert!(raster_auf_zeilen(&takt, 0, 16, 128, 0).is_empty(), "Raster 0");
        assert!(
            raster_auf_zeilen(&takt, 128, 0, 128, 0).is_empty(),
            "0 Zeilen je Takt"
        );
        assert!(
            raster_auf_zeilen(&takt, 128, 16, 0, 0).is_empty(),
            "Laenge 0"
        );
    }

    #[test]
    fn abstaende_in_gruppen() {
        let d = [1.0, 2.0, 3.0, 4.0, 5.0];
        // 5 Abstände in 2er-Gruppen: 2 + 2 + 1, die letzte darf kürzer sein.
        assert_eq!(
            gruppiere_abstaende(&d, 2),
            vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0]],
            "5 Abstände, 2er-Gruppen"
        );
        // Geht die Einteilung auf, gibt es keine leere Restgruppe.
        assert_eq!(
            gruppiere_abstaende(&d[..4], 2),
            vec![vec![1.0, 2.0], vec![3.0, 4.0]]
        );
        // Weniger Abstände als die Gruppengröße: eine einzige Gruppe.
        assert_eq!(
            gruppiere_abstaende(&d, 9),
            vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]
        );
        // 0 ist die Vorgabe „nicht gruppieren“: alles in eine Gruppe.
        assert_eq!(
            gruppiere_abstaende(&d, 0),
            vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]
        );
        // Leer bleibt leer, auch bei Gruppengröße 0.
        assert!(gruppiere_abstaende(&[], 2).is_empty());
        assert!(gruppiere_abstaende(&[], 0).is_empty());
    }

    #[test]
    fn taktfolge_waehlt_gruppen() {
        let gruppen = vec![vec![1.0, 2.0], vec![3.0], vec![4.0, 5.0, 6.0]];
        assert_eq!(
            taktfolge_gruppen(&gruppen, &[0, 2, 0, 1]),
            vec![
                gruppen[0].clone(),
                gruppen[2].clone(),
                gruppen[0].clone(),
                gruppen[1].clone()
            ],
            "Folge G1 G3 G1 G2"
        );
        assert_eq!(gruppen.len(), 3, "die Eingabe bleibt unberührt");
    }

    #[test]
    fn taktfolge_klemmt_zu_grosse_indizes() {
        let gruppen = vec![vec![1.0], vec![2.0]];
        assert_eq!(
            taktfolge_gruppen(&gruppen, &[5, 1, 99]),
            vec![vec![2.0], vec![2.0], vec![2.0]],
            "zu große Indizes werden auf die letzte Gruppe geklemmt"
        );
    }

    #[test]
    fn taktfolge_ohne_gruppen_oder_folge_ist_leer() {
        assert!(taktfolge_gruppen(&[], &[0, 1]).is_empty(), "keine Gruppen");
        let gruppen = vec![vec![1.0]];
        assert!(
            taktfolge_gruppen(&gruppen, &[]).is_empty(),
            "leere Folge"
        );
    }

    #[test]
    fn stichproben_ueber_die_videodauer() {
        let t = stichproben_zeiten(10.0, 30);
        assert_eq!(t.len(), 30, "eine Stichprobe je Abschnitt");
        // Mitte des ersten Abschnitts: 0,5 / 30 * 10 s ≈ 0,1667 s.
        assert!(nah(t[0], 0.1667), "erste Stichprobe: {}", t[0]);
        // Mitte des letzten Abschnitts: 29,5 / 30 * 10 s ≈ 9,8333 s.
        assert!(nah(t[29], 9.8333), "letzte Stichprobe: {}", t[29]);
        assert!(
            t.windows(2).all(|w| w[0] < w[1]),
            "streng aufsteigend: {t:?}"
        );
        assert!(
            t.iter().all(|zeit| *zeit > 0.0 && *zeit < 10.0),
            "alle im Video, nicht auf dem Rand: {t:?}"
        );
        // Eine einzelne Stichprobe ist die Mitte des ganzen Videos.
        let mitte = stichproben_zeiten(10.0, 1);
        assert_eq!(mitte.len(), 1);
        assert!(nah(mitte[0], 5.0), "Mitte: {}", mitte[0]);
    }

    #[test]
    fn stichproben_kaputte_werte_sind_leer() {
        assert!(stichproben_zeiten(10.0, 0).is_empty(), "keine Stichproben");
        assert!(stichproben_zeiten(0.0, 30).is_empty(), "Dauer 0");
        assert!(stichproben_zeiten(-4.0, 30).is_empty(), "negative Dauer");
        assert!(stichproben_zeiten(f32::NAN, 30).is_empty(), "NaN");
        assert!(
            stichproben_zeiten(f32::INFINITY, 30).is_empty(),
            "unendlich"
        );
    }
}
