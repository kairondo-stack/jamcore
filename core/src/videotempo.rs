//! Tempo aus einem **stummen Video** (Plan 2026-10-18, Step 5).
//!
//! Die App zeichnet ihre Sweeps/Stroboskop/Spiralen im Takt; je Videobild
//! liefert der Graumittelwert eine Helligkeits-Zeitreihe. Aus deren
//! Periodizität kommt die BPM — dieselbe Aufgabe wie in [`crate::micbpm`],
//! nur ohne Ton. Weil Videobilder gleichmäßig abgetastet sind, geht es hier
//! direkt über die **Autokorrelation** der Reihe statt über Onsets.
//!
//! Ablauf:
//! 1. **Detrenden**: Mittelwert *und* Ausgleichsgerade abziehen. Videos
//!    driften (Auto-Belichtung, Ein-/Ausblenden); ohne die Gerade bliebe von
//!    einem linearen Helligkeitsverlauf eine Rampe übrig, die wie ein sehr
//!    langsamer Takt aussieht.
//! 2. **Autokorrelation** über alle Lags, die im BPM-Bereich liegen
//!    (Vorgabe 40…220). Normiert auf das Überlappungsfenster, also 1,0 für
//!    einen sauberen Takt und ~0 für Rauschen.
//! 3. **Peak** suchen — nur **echte lokale Maxima**, die deutlich über ihrer
//!    Nachbarschaft liegen (sonst liefert eine monoton fallende
//!    Autokorrelation, z.B. bei reiner Belichtungs-Drift, immer den
//!    Bereichsrand als „Treffer“), kleinster Lag bei gleich starken Peaks,
//!    **zum Fundament herunterzählen** (Vielfache der Periode sind gleich
//!    stark, und ein Vielfaches kann den Lag besser treffen als die Periode
//!    selbst) und mit einer **Parabel** durch die drei Nachbar-Lags
//!    verfeinern (die Feinheit liegt zwischen den Abtastschritten).
//!    Das Herunterzählen rechnet den Lag **gebrochen** (lineare
//!    Interpolation): 128 BPM sind bei 20 fps 9,375 Frames je Schlag; der
//!    ganzzahlige Lag 9 wäre sonst schwächer als sein Dreifaches und die
//!    Auswertung landete bei 42,7 statt 128 BPM.
//! 4. **Aliasing-Schutz**: Liegt schon **unter** dem BPM-Bereich ein fast
//!    gleich starker echter Peak, ist der Treffer ein Vielfaches einer
//!    schnelleren Periode. Bis zu einer Oktave über dem Bereich ist das die
//!    gewollte Halbzeit-Faltung (300 BPM → 150); viel schneller ist Alias —
//!    50-Hz-Netzflimmern bei 20 fps erscheint als 10 Hz = 600 BPM und liefert
//!    bewusst `None` statt „150 BPM“.
//! 5. **Oktav-Regel**: Die Autokorrelation ist bei Vielfachen der Periode
//!    gleich stark — halbe und doppelte Periode sind also gleich gute
//!    Kandidaten. Ist einer davon fast so stark wie der Peak (≥ 90 %),
//!    gewinnt der Kandidat, der **näher an der Mitte des Bereichs** liegt
//!    (arithmetisches Mittel, bei 40…220 also 130 BPM). Damit wird ein
//!    Tempo, dessen halbe Periode aus dem Bereich fällt (z.B. 240 BPM →
//!    120 BPM), sauber in den Bereich gefaltet; umgekehrt wird ein sehr
//!    schnelles Tempo mit gleich starkem Halb-Tempo (z.B. 200 → 100) auf den
//!    musikalisch mittleren Wert gelegt. Die Grenze liegt dort, wo beide
//!    gleich weit von der Mitte entfernt sind: **173,3 BPM** (2·Mitte/1,5) —
//!    darüber gewinnt die halbe Periode. Das ist eine bewusste Heuristik,
//!    keine Messung; der Aufrufer sieht die Konfidenz.
//!
//! Zu kurze Reihen (< 2 s) und konstante Reihen liefern `None`.
//!
//! Der Aufrufer kann die Reihe vor dem Auswerten **ausdünnen** ([`ausduennen`],
//! nur jeden n-ten Wert): die Abtastrate sinkt auf `fps / n`, und durch
//! Aliasing landet ein viel zu schnelles Muster im BPM-Bereich (Prinzip
//! Stroboskop). Als `fps` gehört dann die effektive Rate übergeben — bei
//! Faktor 1,875 fps und weniger kollabiert der Lag-Bereich, dann liefert die
//! Auswertung bewusst `None`.
//!
//! Für die Genre-Bänder belegt der Aufrufer das **erkannte** Tempo mit einem
//! Bruch-Faktor ([`faktoren`], [`tempo_mit_faktor`]): die früheren ÷2/×2-Knöpfe
//! (Ausdünnen) ändern die Abtastrate, nicht das Tempo — die av4-Messung
//! 2026-10-19 zeigt, dass die nützlichen Faktoren Brüche mit Nenner 3–8 sind
//! (×5/8, ×2/3, ×5/6, ×7/8), nicht Zweierpotenzen.

/// Untere Grenze des Vorgabe-Bereichs (BPM).
pub const MIN_BPM: f32 = 40.0;
/// Obere Grenze des Vorgabe-Bereichs (BPM).
pub const MAX_BPM: f32 = 220.0;

/// Kürzere Reihen als diese Dauer liefern nichts (zu wenig Perioden).
const MINDEST_SEKUNDEN: f32 = 2.0;

/// Ab dieser Höhe der normierten Autokorrelation gilt ein Peak als Takt.
/// Reines Rauschen bleibt darunter (bei den üblichen Reihenlängen).
const MINDEST_PEAK: f32 = 0.3;

/// Ab diesem Anteil des Peaks gilt ein kürzerer Lag als **Fundament**
/// (Herunterzählen der Vielfachen, siehe [`bpm_aus_reihe`]).
///
/// Kleiner als [`OKTAV_ANTEIL`], weil ein gebrochener Lag durch das
/// Abtastraster geschwächt wird, ohne dass der Takt schlechter wäre: ein
/// 2-Frame-Puls bei 96 BPM und 20 fps hat seine Periode bei 12,5 Frames, und
/// die Korrelation dort liegt gemessen bei 0,83 gegenüber 0,99 beim doppelten
/// Lag (25). Mit 0,9 blieb die Auswertung auf 48 BPM stehen; 0,8 trifft das
/// Fundament (Sweep 60…200 BPM, 2026-10-21). Rauschen bleibt draußen, weil
/// vorher schon [`MINDEST_PEAK`] und die Prominenz geprüft werden.
const FUNDAMENT_ANTEIL: f32 = 0.8;

/// Ab diesem Anteil des Peaks gilt die halbe/doppelte Periode als „fast
/// gleich stark" (Oktav-Regel, siehe Modulkopf).
const OKTAV_ANTEIL: f32 = 0.9;

/// Gleich starke Peaks: erst ab diesem relativen Vorsprung gewinnt ein
/// späterer (längerer) Lag. So bleibt bei Vielfachen der Periode der
/// **kleinste** Lag stehen — die Oktav-Regel entscheidet danach.
const PEAK_TOLERANZ: f32 = 1e-4;

/// Ein Treffer muss ein **echtes lokales Maximum** sein *und* deutlich über
/// dem Mittel seiner Nachbarschaft liegen: Peak ≥ [`PROMINENZ`] × Mittel der
/// (bis zu) [`PROMINENZ_NACHBARN`] Nachbarn je Seite.
const PROMINENZ: f32 = 1.15;

/// So viele Lags links und rechts zählen als Nachbarschaft (am Rand des
/// Bereichs stehen weniger zur Verfügung — die vorhandenen zählen).
const PROMINENZ_NACHBARN: usize = 5;

/// Kleinster Höhenunterschied für ein lokales Maximum. Links muss der
/// Nachbar echt tiefer liegen; rechts genügt „nicht höher“, damit ein Plateau
/// (Periode genau zwischen zwei Lags, z.B. 160 BPM bei 20 fps) nicht
/// durchfällt — der kleinere Lag gewinnt dann, wie bisher bei gleich starken
/// Peaks.
const PEAK_EPS: f32 = 1e-6;

/// Graumittelwert eines RGBA-Puffers (0…255, wie `yuv420_to_rgba` ihn legt).
///
/// Das ist die Zeitreihe je Videobild: „wie hell ist das Bild gerade".
/// Leerer Puffer → 0.0; ein halbes Pixel am Ende wird ignoriert.
pub fn graumittel(rgba: &[u8]) -> f32 {
    let mut summe = 0.0f64;
    let mut anzahl = 0u64;
    for pixel in rgba.chunks_exact(4) {
        summe += (pixel[0] as f64 + pixel[1] as f64 + pixel[2] as f64) / 3.0;
        anzahl += 1;
    }
    if anzahl == 0 {
        0.0
    } else {
        (summe / anzahl as f64) as f32
    }
}

/// Dauer als Log-/Statustext: „12.3 s“ oder „unbekannt“.
///
/// Warum im Kern: die Android-Brücke liefert nur `Option<f32>` (Sekunden) aus
/// `MediaMetadataRetriever`; die Formatierung ist die einzige Logik dazwischen
/// und damit ohne Gerät prüfbar. Werte bis 0,05 s gelten als unbekannt — der
/// Retriever liefert bei kaputten Containern 0, und „0.0 s“ wäre irreführend.
pub fn dauer_text(dauer: Option<f32>) -> String {
    match dauer {
        Some(s) if s.is_finite() && s > 0.05 => format!("{s:.1} s"),
        _ => "unbekannt".to_string(),
    }
}

/// Helligkeits-Zeitreihe (je Frame) → BPM-Schätzung.
///
/// `fps` ist die **Abtastrate der Reihe** (nicht zwingend die Bildrate des
/// Videos: die App nimmt je Anzeigetakt höchstens ein Bild auf). Zu kurze
/// (< 2 s) und konstante Reihen liefern `None`, ebenso ein Peak unter
/// [`MINDEST_PEAK`] (Rauschen ohne Takt) und eine Reihe ohne **echtes lokales
/// Maximum** mit Prominenz ([`PROMINENZ`]) — eine monoton fallende
/// Autokorrelation (Belichtungs-Drift) liefert also „nichts erkannt" statt
/// des Bereichsrands.
pub fn bpm_aus_reihe(werte: &[f32], fps: f32, min_bpm: f32, max_bpm: f32) -> Option<f32> {
    if !fps.is_finite() || fps <= 0.0 || werte.len() < 2 {
        return None;
    }
    if !min_bpm.is_finite() || !max_bpm.is_finite() || min_bpm <= 0.0 || max_bpm <= min_bpm {
        return None;
    }
    if werte.iter().any(|v| !v.is_finite()) {
        return None;
    }
    if werte.len() as f32 / fps < MINDEST_SEKUNDEN {
        return None;
    }
    let x = detrenden(werte);
    // Konstante Reihe: nach dem Detrenden steht nur noch ~0 da.
    let staerke = x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32;
    if staerke <= 1e-12 {
        return None;
    }
    let (lag_min, lag_max) = lag_bereich(x.len(), fps, min_bpm, max_bpm)?;
    // r ab Lag 1: die kurzen Lags **unter** `lag_min` braucht der
    // Aliasing-Schutz weiter unten; Lag 1 ist selbst kein Kandidat, dient aber
    // als linker Nachbar von Lag 2 (sonst wäre ein 2-Frame-Alias unsichtbar).
    const R_VON: usize = 1;
    let r: Vec<f32> = (R_VON..=lag_max)
        .map(|lag| autokorrelation(&x, lag))
        .collect();
    // Kandidaten sind nur **echte lokale Maxima**. Ohne diese Bedingung nimmt
    // eine monoton fallende Autokorrelation (Belichtungs-Drift, Video ohne
    // Takt) immer `lag_min` — den linken Rand des Bereichs, also das
    // schnellste erlaubte Tempo (av4-Messung 2026-10-19: 199,6 BPM bei
    // 40…220, 300,0 BPM bei 40…300 — der Wert wanderte mit dem Rand mit).
    let mut bester_lag: Option<usize> = None;
    for (i, wert) in r.iter().enumerate() {
        let lag = R_VON + i;
        if lag < lag_min || !lokales_maximum(&r, i) {
            continue;
        }
        match bester_lag {
            None => bester_lag = Some(lag),
            Some(bisher) => {
                if *wert > r[bisher - R_VON] * (1.0 + PEAK_TOLERANZ) {
                    bester_lag = Some(lag);
                }
            }
        }
    }
    let Some(bester_lag) = bester_lag else {
        return None;
    };
    // Ein einzelner flacher Buckel auf einer fallenden Kurve ist noch kein
    // Takt: der Peak muss deutlich über seiner Nachbarschaft liegen.
    if !ist_prominent(&r, bester_lag - R_VON) {
        return None;
    }
    let mut bester_r = r[bester_lag - R_VON];
    if bester_r < MINDEST_PEAK {
        return None;
    }
    // Aliasing-Schutz: schon **unterhalb** des Bereichs ein fast gleich
    // starker, echter Peak? Dann ist der gefundene Lag ein Vielfaches einer
    // schnelleren Periode. Bis zu einer Oktave über `max_bpm` ist das die
    // gewollte Halbzeit-Faltung (300 BPM → 150, siehe Bereichstest); viel
    // schneller ist dagegen Alias — z.B. 50-Hz-Netzflimmern bei 20 fps
    // (Alias 10 Hz = 600 BPM), das sonst als „150 BPM“ durchginge. Dort ist
    // „nichts erkannt“ ehrlicher als ein Vielfaches des Alias.
    for (i, wert) in r.iter().enumerate() {
        let lag = R_VON + i;
        if lag < 2 || lag >= lag_min {
            continue;
        }
        if !lokales_maximum(&r, i) || *wert < bester_r * OKTAV_ANTEIL {
            continue;
        }
        if 60.0 * fps / lag as f32 > 2.0 * max_bpm {
            return None;
        }
        break;
    }
    // Fundament statt Vielfaches: bei einem Takt sind alle Vielfachen der
    // Periode gleich stark — und je nach Abtastraster trifft ein Vielfaches
    // den ganzzahligen Lag besser als die Periode selbst. Deshalb von unten
    // herunterzählen: jeder kürzere Lag im Bereich, der fast so stark ist wie
    // der Peak, ist der bessere Kandidat.
    //
    // Der Lag wird dabei **gebrochen** gerechnet (lineare Interpolation):
    // 128 BPM bei 20 fps sind 9,375 Frames je Schlag, und der ganzzahlige
    // Lag 9 unterschätzt die Stärke so weit, dass das Dreifache (Lag 28)
    // gewinnt (gemessen: 42,7 statt 128 BPM). Mit 28/3 = 9,333 ist die
    // Korrelation wieder so stark wie der Peak.
    let mut bester_lag = bester_lag as f32;
    let mut k = 2usize;
    while bester_lag > lag_min as f32 {
        let lag = bester_lag / k as f32;
        if lag < lag_min as f32 {
            break;
        }
        let r = autokorrelation_f(&x, lag);
        if r >= bester_r * FUNDAMENT_ANTEIL {
            bester_lag = lag;
            bester_r = r;
            k = 2;
        } else {
            k += 1;
            if k > bester_lag.ceil() as usize {
                break;
            }
        }
    }
    if bester_r < MINDEST_PEAK {
        return None;
    }
    // Oktav-Regel: halbe und doppelte Periode sind gleich gute Kandidaten.
    let mitte = 0.5 * (min_bpm + max_bpm);
    let mut gewaehlt = 60.0 * fps / bester_lag;
    let mut naehe = (gewaehlt - mitte).abs();
    for kandidat in [gewaehlt * 0.5, gewaehlt * 2.0] {
        if !(min_bpm..=max_bpm).contains(&kandidat) {
            continue;
        }
        let lag = 60.0 * fps / kandidat;
        if lag < 2.0 || lag >= x.len() as f32 {
            continue;
        }
        let r = autokorrelation_f(&x, lag);
        if r >= bester_r * OKTAV_ANTEIL && (kandidat - mitte).abs() < naehe {
            gewaehlt = kandidat;
            naehe = (kandidat - mitte).abs();
        }
    }
    // Feinschliff: ein Lag aus dem **ganzzahligen** Raster liegt oft zwischen
    // zwei Lags (Parabel durch die Nachbarn). Ein gebrochener Lag aus der
    // Fundament-Suche ist dagegen schon der exakte Kandidat — dort würde die
    // Parabel nur wieder auf das Raster ziehen.
    let lag = 60.0 * fps / gewaehlt;
    let fein = if (lag - lag.round()).abs() < 1e-3 {
        feinlag(&x, lag.round() as usize, lag_min, lag_max)
    } else {
        lag
    };
    Some(60.0 * fps / fein)
}

/// Bequem: aus einer Liste von Frame-Mittelwerten, Bereich 40…220 BPM.
pub fn bpm_aus_frames(mittelwerte: &[f32], fps: f32) -> Option<f32> {
    bpm_aus_reihe(mittelwerte, fps, MIN_BPM, MAX_BPM)
}

// ==== FAKTOR-KNÖPFE (Genre-Brüche) ========================================
// Die av4-Messung (2026-10-19) zeigte, wohin die Genre-Faktoren greifen
// müssen: bei der typischen Ausgabe um 200 BPM (früher der Bereichsrand,
// siehe Modulkopf) treffen Brüche mit Nenner 3–8 die Bänder House/Groove
// 120–128, Techno 120–140, Breakbeat 160–175 und DnB 170–180. Zweierpotenzen
// (×1/2, ×1/3, ×1/4) sind bei einem Ausgabebereich bis 220 dagegen
// wirkungslos — sie bräuchten 240–720 BPM Eingabe. ×2/×3 helfen umgekehrt,
// wenn die Auswertung tief faltet (60 → 120/180). Der Faktor liegt **hinter**
// `bpm_aus_frames`: „nichts erkannt“ bleibt „nichts erkannt“.

/// Faktor als Bruch: `zaehler / nenner` (ein Nenner 0 zählt wie 1).
pub fn faktor_bruchteil(zaehler: u32, nenner: u32) -> f32 {
    zaehler as f32 / nenner.max(1) as f32
}

/// Die Faktor-Knöpfe des Bild-Tabs: (Zähler, Nenner, Genre-Hinweis).
///
/// ×1 steht vorn — die Vorgabe verstellt kein Tempo ungefragt. Die Reihenfolge
/// ist die Reihenfolge der Knöpfe in der Oberfläche.
pub fn faktoren() -> &'static [(u32, u32, &'static str)] {
    &[
        (1, 1, "Vorgabe"),
        (5, 8, "House/Groove 120–128"),
        (2, 3, "Techno 120–140"),
        (5, 6, "Breakbeat 160–175"),
        (7, 8, "Drum and Bass 170–180"),
        (2, 1, "tiefe Ausgabe: 60 → 120"),
        (3, 1, "tiefe Ausgabe: 60 → 180"),
    ]
}

/// Erkanntes Tempo mit einem Bruch-Faktor belegen und auf den Suchbereich
/// ([`MIN_BPM`]…[`MAX_BPM`]) klemmen.
///
/// Nur für ein **erkanntes** Tempo gedacht: liefert [`bpm_aus_reihe`] `None`,
/// reicht der Aufrufer das „nichts erkannt“ unverändert durch.
pub fn tempo_mit_faktor(bpm: f32, zaehler: u32, nenner: u32) -> f32 {
    (bpm * faktor_bruchteil(zaehler, nenner)).clamp(MIN_BPM, MAX_BPM)
}

// ==== ZEITLUPE: Ausdünnen (Aliasing) ======================================
// Die Muster im Kamerabild sind oft viel zu schnell für den BPM-Bereich
// (gemessen 2026-10-19: Perioden von 5–36 ms ≈ 1 660–11 900 BPM). Wertet man
// nur jeden n-ten Frame aus, sinkt die Abtastrate auf fps/n, und durch
// **Aliasing** erscheint die Periode langsamer — dasselbe Prinzip wie beim
// Stroboskop. Die erwartete Ersatzfrequenz ist der Abstand der Musterfrequenz
// zum nächsten Vielfachen von fps/n.

/// Jeden n-ten Wert nehmen (n ≥ 1; `n = 0` zählt wie 1). Zeitstempel bleiben
/// außen vor — nur die Reihe wird ausgedünnt, die **effektive Abtastrate**
/// ist `fps / n` (siehe [`bpm_aus_reihe`]).
pub fn ausduennen(werte: &[f32], n: usize) -> Vec<f32> {
    werte.iter().step_by(n.max(1)).copied().collect()
}

/// Faktor auf die erlaubten Stufen klemmen: 1, 2, 4, …, 128. Andere Werte
/// werden auf die nächstkleinere Zweierpotenz abgerundet (0 → 1).
pub fn klemme_faktor(n: usize) -> usize {
    let mut faktor = 1usize;
    while faktor < 128 && faktor * 2 <= n {
        faktor *= 2;
    }
    faktor
}

/// Kurzform des Faktors für Log und Status: „1x", „1/8", „1/128".
pub fn faktor_kuerzel(n: usize) -> String {
    let n = klemme_faktor(n);
    if n == 1 {
        "1x".to_string()
    } else {
        format!("1/{n}")
    }
}

/// Anzeige des Faktors: „1x (jeder Frame)", „1/8 (jeder 8. Frame)".
pub fn faktor_text(n: usize) -> String {
    let n = klemme_faktor(n);
    if n == 1 {
        "1x (jeder Frame)".to_string()
    } else {
        format!("1/{n} (jeder {n}. Frame)")
    }
}

/// Konfidenz 0…1: Höhe des Autokorrelations-Peaks bei `bpm` relativ zu den
/// übrigen Lags des Bereichs („Nachbarn").
///
/// 1,0 = ein sauberer Takt (Sinus/Klickzug), ~0 = Rauschen. Die unmittelbare
/// Umgebung des Peaks bleibt außen vor, weil die Autokorrelation dort bei
/// jeder glatten Reihe hoch ist.
pub fn konfidenz(werte: &[f32], fps: f32, bpm: f32) -> f32 {
    if !fps.is_finite() || fps <= 0.0 || !bpm.is_finite() || bpm <= 0.0 || werte.len() < 2 {
        return 0.0;
    }
    if werte.iter().any(|v| !v.is_finite()) {
        return 0.0;
    }
    let x = detrenden(werte);
    let Some((lag_min, lag_max)) = lag_bereich(x.len(), fps, MIN_BPM, MAX_BPM) else {
        return 0.0;
    };
    // Nur ein Tempo aus dem Bereich hat einen Lag, der hier zählt.
    let peak_lag = (60.0 * fps / bpm).round() as usize;
    if peak_lag < lag_min || peak_lag > lag_max {
        return 0.0;
    }
    let peak = autokorrelation(&x, peak_lag);
    if peak <= 0.0 {
        return 0.0;
    }
    let mut summe = 0.0f32;
    let mut anzahl = 0usize;
    for lag in lag_min..=lag_max {
        if (lag as i64 - peak_lag as i64).abs() <= 2 {
            continue;
        }
        summe += autokorrelation(&x, lag).abs();
        anzahl += 1;
    }
    if anzahl == 0 {
        return 0.0;
    }
    let nachbarn = summe / anzahl as f32;
    ((peak - nachbarn) / (1.0 - nachbarn).max(1e-6)).clamp(0.0, 1.0)
}

/// Mittelwert **und** Ausgleichsgerade abziehen (Belichtungs-Drift).
fn detrenden(werte: &[f32]) -> Vec<f32> {
    let n = werte.len() as f64;
    let mittel = werte.iter().map(|v| *v as f64).sum::<f64>() / n;
    let (mut sx, mut sy, mut sxx, mut sxy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (i, v) in werte.iter().enumerate() {
        let x = i as f64;
        let y = *v as f64 - mittel;
        sx += x;
        sy += y;
        sxx += x * x;
        sxy += x * y;
    }
    let nenner = n * sxx - sx * sx;
    let steigung = if nenner.abs() > 1e-12 {
        (n * sxy - sx * sy) / nenner
    } else {
        0.0
    };
    let achsabschnitt = (sy - steigung * sx) / n;
    werte
        .iter()
        .enumerate()
        .map(|(i, v)| (*v as f64 - mittel - (steigung * i as f64 + achsabschnitt)) as f32)
        .collect()
}

/// Normierte Autokorrelation bei `lag` (Kosinus-Ähnlichkeit über die
/// Überlappung): 1,0 = identisch, 0,0 = unkorreliert, −1,0 = umgekehrt.
fn autokorrelation(werte: &[f32], lag: usize) -> f32 {
    if lag == 0 || lag >= werte.len() {
        return 0.0;
    }
    let n = werte.len() - lag;
    let (mut summe, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        let a = werte[i] as f64;
        let b = werte[i + lag] as f64;
        summe += a * b;
        aa += a * a;
        bb += b * b;
    }
    let nenner = (aa * bb).sqrt();
    if nenner <= 1e-12 {
        0.0
    } else {
        (summe / nenner) as f32
    }
}

/// Normierte Autokorrelation bei **gebrochenem** Lag: zwischen den
/// Abtastwerten wird linear interpoliert (`frac` = Nachkommaanteil).
///
/// Warum das nötig ist: ein Takt von 128 BPM bei 20 Bildern je Sekunde hat
/// 9,375 Frames je Schlag. Der ganzzahlige Lag 9 misst dann nur die Schläge,
/// deren Phase zufällig passt, und ist deutlich schwächer als das Dreifache
/// (Lag 28) — die Auswertung fiele auf 42,7 BPM. Bei exakt ganzzahligem Lag
/// wird [`autokorrelation`] benutzt (gleiche Werte wie bisher).
fn autokorrelation_f(werte: &[f32], lag: f32) -> f32 {
    if !lag.is_finite() || lag < 1.0 {
        return 0.0;
    }
    let ganz = lag.floor();
    let frac = lag - ganz;
    let i0 = ganz as usize;
    if frac < 1e-6 {
        return autokorrelation(werte, i0);
    }
    if i0 + 1 >= werte.len() {
        return 0.0;
    }
    let n = werte.len() - i0 - 1;
    let (mut summe, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        let a = werte[i] as f64;
        let b = werte[i + i0] as f64 * (1.0 - frac as f64)
            + werte[i + i0 + 1] as f64 * frac as f64;
        summe += a * b;
        aa += a * a;
        bb += b * b;
    }
    let nenner = (aa * bb).sqrt();
    if nenner <= 1e-12 {
        0.0
    } else {
        (summe / nenner) as f32
    }
}

/// Echte lokale Maxima der r-Kurve (`r[i]` gehört zu `lag_min + i`):
/// links streng höher (mit ε), rechts nicht höher — der erste Lag eines
/// Plateaus gewinnt. Die **Ränder** des Bereichs sind keine Kandidaten: ihnen
/// fehlt ein Nachbar, und genau deshalb liefert eine monoton fallende Kurve
/// nicht mehr `lag_min` als „Treffer“.
fn lokales_maximum(r: &[f32], i: usize) -> bool {
    i > 0 && i + 1 < r.len() && r[i] > r[i - 1] + PEAK_EPS && r[i] + PEAK_EPS >= r[i + 1]
}

/// Liegt der Peak deutlich über dem Mittel seiner Nachbarschaft
/// ([`PROMINENZ`], bis zu [`PROMINENZ_NACHBARN`] Lags je Seite)? Am Rand des
/// Bereichs stehen weniger Nachbarn zur Verfügung — die vorhandenen zählen.
fn ist_prominent(r: &[f32], i: usize) -> bool {
    let von = i.saturating_sub(PROMINENZ_NACHBARN);
    let bis = (i + PROMINENZ_NACHBARN).min(r.len() - 1);
    let (mut summe, mut anzahl) = (0.0f32, 0usize);
    for (j, wert) in r.iter().enumerate().take(bis + 1).skip(von) {
        if j != i {
            summe += *wert;
            anzahl += 1;
        }
    }
    anzahl > 0 && r[i] >= PROMINENZ * (summe / anzahl as f32)
}

/// Lags, die in [min_bpm, max_bpm] liegen. Kürzester Lag = schnellstes Tempo.
fn lag_bereich(n: usize, fps: f32, min_bpm: f32, max_bpm: f32) -> Option<(usize, usize)> {
    if fps <= 0.0 || min_bpm <= 0.0 || max_bpm <= min_bpm {
        return None;
    }
    let lag_min = ((60.0 * fps / max_bpm).ceil() as usize).max(2);
    let lag_max = (60.0 * fps / min_bpm).floor() as usize;
    if lag_min >= n {
        return None;
    }
    // Mindestens zwei Werte Überlappung, sonst ist die Korrelation Zufall.
    let lag_max = lag_max.min(n - 2);
    if lag_max <= lag_min {
        return None;
    }
    Some((lag_min, lag_max))
}

/// Parabel durch den Peak und seine beiden Nachbarn: die feine Periode liegt
/// zwischen den Abtastschritten.
fn feinlag(werte: &[f32], lag: usize, lag_min: usize, lag_max: usize) -> f32 {
    if lag <= lag_min || lag >= lag_max || lag + 1 >= werte.len() {
        return lag as f32;
    }
    let (a, b, c) = (
        autokorrelation(werte, lag - 1),
        autokorrelation(werte, lag),
        autokorrelation(werte, lag + 1),
    );
    let nenner = a - 2.0 * b + c;
    if nenner.abs() < 1e-9 {
        return lag as f32;
    }
    let delta = 0.5 * (a - c) / nenner;
    if !delta.is_finite() || delta.abs() > 1.0 {
        return lag as f32;
    }
    lag as f32 + delta
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sinus um 0,5 mit Amplitude 0,5 — wie eine weiche Helligkeitswelle.
    fn sinus(bpm: f32, fps: f32, sekunden: f32) -> Vec<f32> {
        let n = (fps * sekunden) as usize;
        let f = bpm / 60.0;
        (0..n)
            .map(|i| {
                let t = i as f32 / fps;
                0.5 + 0.5 * (2.0 * std::f32::consts::PI * f * t).sin()
            })
            .collect()
    }

    /// Klickzug (Helligkeitsblitze) mit ganzzahliger Periode in Abtastschritten
    /// — wie ein Stroboskop im Takt.
    fn klicks(bpm: f32, fps: f32, sekunden: f32) -> Vec<f32> {
        let n = (fps * sekunden) as usize;
        let periode = 60.0 * fps / bpm;
        assert!(
            (periode - periode.round()).abs() < 1e-3,
            "Klickzug braucht eine ganzzahlige Periode ({periode})"
        );
        let periode = periode.round() as usize;
        let mut v = vec![0.1f32; n];
        let mut i = 0;
        while i < n {
            v[i] = 1.0;
            if i + 1 < n {
                v[i + 1] = 0.7;
            }
            i += periode;
        }
        v
    }

    fn rauschen(n: usize) -> Vec<f32> {
        let mut state = 0x1234_5678u32;
        (0..n)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state as f32 / u32::MAX as f32) - 0.5
            })
            .collect()
    }

    /// Weißes Rauschen glätten (gleitendes Mittel über `fenster` Werte):
    /// eine träge Reihe mit **monoton fallender** Autokorrelation im
    /// Lag-Bereich — die Form der av4-Graumittel-Reihe (2026-10-19).
    fn glattes_rauschen(n: usize, fenster: usize) -> Vec<f32> {
        let roh = rauschen(n);
        (0..n)
            .map(|i| {
                let von = i.saturating_sub(fenster / 2);
                let bis = (i + fenster / 2 + 1).min(n);
                roh[von..bis].iter().sum::<f32>() / (bis - von) as f32
            })
            .collect()
    }

    /// AR(1)-Reihe (träge Drift, wie eine Belichtungs-Drift), auf Varianz 1
    /// normiert: x[i] = a·x[i−1] + Rauschen, Autokorrelation ≈ a^lag.
    fn drift_reihe(n: usize, a: f32) -> Vec<f32> {
        let roh = rauschen(n);
        let mut v = vec![0.0f32; n];
        for i in 1..n {
            v[i] = a * v[i - 1] + roh[i];
        }
        let mittel = v.iter().sum::<f32>() / n as f32;
        let varianz = v.iter().map(|x| (x - mittel) * (x - mittel)).sum::<f32>() / n as f32;
        let s = varianz.sqrt().max(1e-9);
        v.iter().map(|x| (x - mittel) / s).collect()
    }

    /// Einzelne Helligkeitsblitze (ein Abtastwert je Periode, sonst 0,1):
    /// scharfe Autokorrelations-Peaks im Gegensatz zum weichen Sinus.
    fn blitze(bpm: f32, fps: f32, sekunden: f32) -> Vec<f32> {
        let n = (fps * sekunden) as usize;
        let periode = 60.0 * fps / bpm;
        assert!(
            (periode - periode.round()).abs() < 1e-3,
            "Blitze brauchen eine ganzzahlige Periode ({periode})"
        );
        let periode = periode.round() as usize;
        let mut v = vec![0.1f32; n];
        let mut i = 0;
        while i < n {
            v[i] = 1.0;
            i += periode;
        }
        v
    }

    #[test]
    fn sinus_mit_120_bpm_wird_erkannt() {
        let werte = sinus(120.0, 30.0, 12.0);
        let bpm = bpm_aus_frames(&werte, 30.0).expect("120 BPM im Sinus");
        assert!((bpm - 120.0).abs() < 1.5, "erwartet 120, war {bpm}");
        let k = konfidenz(&werte, 30.0, bpm);
        assert!(k > 0.5, "sauberer Sinus muss überzeugen, war {k}");
    }

    #[test]
    fn sinus_mit_90_bpm() {
        let werte = sinus(90.0, 30.0, 12.0);
        let bpm = bpm_aus_frames(&werte, 30.0).expect("90 BPM im Sinus");
        assert!((bpm - 90.0).abs() < 1.5, "erwartet 90, war {bpm}");
    }

    #[test]
    fn rauschen_ohne_takt_liefert_nichts() {
        // 15 s bei 30 fps: genug Material, aber keine Periode.
        assert!(bpm_aus_frames(&rauschen(450), 30.0).is_none());
    }

    #[test]
    fn rand_der_bereich_wird_nicht_geliefert() {
        // av4-Messung (2026-10-19): die Graumittel-Reihe des taktlosen Videos
        // hat eine monoton fallende Autokorrelation. Der alte Code nahm das
        // globale Maximum — bei fallender Kurve also immer den **linken Rand**
        // des Lag-Bereichs: 199,6 BPM bei 40…220, 300,0 BPM bei 40…300. Der
        // Wert wanderte also mit dem Rand mit, war aber kein Takt.
        let werte = glattes_rauschen(600, 20);
        let x = detrenden(&werte);
        let (lag_min, lag_max) = lag_bereich(x.len(), 30.0, MIN_BPM, MAX_BPM).expect("Bereich");
        let rand = autokorrelation(&x, lag_min);
        assert!(
            rand >= MINDEST_PEAK,
            "Vorbedingung: der Rand wäre ein Treffer gewesen (r = {rand})"
        );
        for lag in (lag_min + 1)..=lag_max {
            assert!(
                autokorrelation(&x, lag) <= rand,
                "Vorbedingung: r fällt monoton, lag {lag}"
            );
        }
        assert!(
            bpm_aus_frames(&werte, 30.0).is_none(),
            "der Bereichsrand ist kein Takt"
        );
    }

    #[test]
    fn taktloses_rauschen_ist_none() {
        // Nur Helligkeitsdrift: av4 fand „eine globale Helligkeitsdrift, keine
        // Szene“ — selbst der unbewegte Himmel lieferte 200 BPM. Hier die
        // beiden Formen ohne jeden Takt: Random Walk (Drift) und glattes
        // Rauschen (Tiefpass).
        let mut stand = 0.0f32;
        let drift: Vec<f32> = rauschen(600)
            .into_iter()
            .map(|r| {
                stand += r;
                stand
            })
            .collect();
        assert!(
            bpm_aus_frames(&drift, 30.0).is_none(),
            "Helligkeitsdrift ist kein Takt"
        );
        assert!(
            bpm_aus_frames(&glattes_rauschen(600, 40), 30.0).is_none(),
            "glattes Rauschen ist kein Takt"
        );
    }

    #[test]
    fn lokales_maximum_gegen_rand() {
        // Künstliche Reihe, in der der Bereichsrand **stärker** korreliert ist
        // als das echte Maximum (träge Drift + kleine Blitze bei 60 BPM).
        // Geliefert werden darf nur das echte Maximum — oder nichts; nie der
        // Rand (der wäre ~200 BPM).
        let drift = drift_reihe(450, 0.98);
        let blitz = blitze(60.0, 30.0, 15.0);
        let reihe: Vec<f32> = drift
            .iter()
            .zip(blitz.iter())
            .map(|(d, b)| d + 2.6 * b)
            .collect();
        let x = detrenden(&reihe);
        let (lag_min, _) = lag_bereich(x.len(), 30.0, MIN_BPM, MAX_BPM).expect("Bereich");
        let echt = 30; // 60 BPM bei 30 fps
        assert!(
            autokorrelation(&x, lag_min) > autokorrelation(&x, echt),
            "Vorbedingung: der Rand ist stärker ({:.3} vs {:.3})",
            autokorrelation(&x, lag_min),
            autokorrelation(&x, echt)
        );
        match bpm_aus_frames(&reihe, 30.0) {
            None => {}
            Some(bpm) => assert!(
                (bpm - 60.0).abs() < 3.0,
                "nur das echte Maximum oder nichts, war {bpm}"
            ),
        }
    }

    #[test]
    fn konstante_reihe_ist_none() {
        assert!(bpm_aus_frames(&vec![0.5f32; 300], 30.0).is_none());
        assert!(bpm_aus_frames(&vec![0.0f32; 300], 30.0).is_none());
    }

    #[test]
    fn zu_kurze_reihe_ist_none() {
        // 1 s bei 30 fps (< 2 s Mindestdauer).
        assert!(bpm_aus_frames(&sinus(120.0, 30.0, 1.0), 30.0).is_none());
        // Gerade noch zu kurz: 59 Bilder bei 30 fps sind 1,97 s.
        assert!(bpm_aus_frames(&sinus(120.0, 30.0, 59.0 / 30.0), 30.0).is_none());
        // Leer und ein einzelner Wert stürzen nicht ab.
        assert!(bpm_aus_frames(&[], 30.0).is_none());
        assert!(bpm_aus_frames(&[0.5], 30.0).is_none());
        assert!(bpm_aus_reihe(&sinus(120.0, 30.0, 12.0), 0.0, 40.0, 220.0).is_none());
        assert!(bpm_aus_reihe(&sinus(120.0, 30.0, 12.0), 30.0, 220.0, 40.0).is_none());
        assert!(bpm_aus_reihe(&[0.0, f32::NAN, 1.0], 30.0, 40.0, 220.0).is_none());
    }

    #[test]
    fn fps_aendert_das_ergebnis_nicht() {
        // Dieselbe Wellenform (128 BPM) mit 30 und 60 fps abgetastet.
        let bei_30 = sinus(128.0, 30.0, 12.0);
        let bei_60 = sinus(128.0, 60.0, 12.0);
        let a = bpm_aus_frames(&bei_30, 30.0).expect("128 BPM bei 30 fps");
        let b = bpm_aus_frames(&bei_60, 60.0).expect("128 BPM bei 60 fps");
        assert!((a - 128.0).abs() < 1.5, "30 fps: erwartet 128, war {a}");
        assert!((b - 128.0).abs() < 1.5, "60 fps: erwartet 128, war {b}");
        assert!(
            (a - b).abs() < 0.5,
            "fps darf das Tempo nicht verschieben: {a} vs {b}"
        );
    }

    #[test]
    fn fundamental_statt_vielfachem() {
        // Der ganzzahlige Lag trifft nicht immer die Periode: bei 127,8 BPM
        // und 20 fps (so läuft das Video in der App) liegt das Dreifache
        // (42,6 BPM) näher am Lag-Raster als die Periode selbst. Gefunden
        // werden muss trotzdem das Fundament.
        for (bpm, fps) in [(127.8f32, 20.0f32), (127.8, 30.0), (128.0, 20.0)] {
            let werte = sinus(bpm, fps, 24.0);
            let erkannt = bpm_aus_frames(&werte, fps).expect("Sinus muss erkannt werden");
            assert!(
                (erkannt - bpm).abs() < 1.5,
                "{bpm} BPM bei {fps} fps: erwartet {bpm}, war {erkannt}"
            );
        }
    }

    #[test]
    fn app_takte_bei_20_bildern_je_sekunde() {
        // Die Abtastrate der App ist die Anzeige (~20 Hz), nicht die Bildrate
        // des Videos: die üblichen Tempi müssen dabei stimmen.
        for bpm in [90.0f32, 120.0, 128.0, 140.0, 160.0, 170.0] {
            let werte = sinus(bpm, 20.0, 20.0);
            let erkannt = bpm_aus_frames(&werte, 20.0).expect("Takt muss erkannt werden");
            assert!((erkannt - bpm).abs() < 1.5, "erwartet {bpm}, war {erkannt}");
        }
    }

    #[test]
    fn halbzeit_und_doppelzeit() {
        // Oktav-Regel: die Autokorrelation ist bei Vielfachen der Periode
        // gleich stark. Liegt die wahre Periode (halbe Periode = doppeltes
        // Tempo) ausserhalb des Bereichs, gewinnt das Vielfache: 240 BPM
        // (Periode 0,25 s) → 120 BPM.
        let schnell = klicks(240.0, 60.0, 12.0);
        let bpm = bpm_aus_frames(&schnell, 60.0).expect("240 BPM als Klickzug");
        assert!(
            (bpm - 120.0).abs() < 2.0,
            "240 BPM gehört auf 120 gefaltet, war {bpm}"
        );

        // Doppelte Periode: 200 BPM hat bei 100 BPM einen gleich starken
        // Kandidaten. 100 liegt näher an der Bereichsmitte (130) — die Regel
        // nimmt ihn (dokumentierte Heuristik, kein Messfehler).
        let hoch = klicks(200.0, 60.0, 12.0);
        let bpm = bpm_aus_frames(&hoch, 60.0).expect("200 BPM als Klickzug");
        assert!(
            (bpm - 100.0).abs() < 2.0,
            "200 BPM gehört auf 100 gelegt, war {bpm}"
        );

        // Ein mittleres Tempo bleibt stehen: 120 BPM hat bei 60 BPM einen
        // gleich starken Kandidaten, 120 liegt aber näher an der Mitte.
        let mittel = klicks(120.0, 60.0, 12.0);
        let bpm = bpm_aus_frames(&mittel, 60.0).expect("120 BPM als Klickzug");
        assert!(
            (bpm - 120.0).abs() < 2.0,
            "120 BPM darf nicht fallen, war {bpm}"
        );
    }

    #[test]
    fn drift_und_rampe_liefern_nichts() {
        // Auto-Belichtung: monotone Helligkeitsänderung ohne Takt.
        let rampe: Vec<f32> = (0..450).map(|i| i as f32 / 450.0).collect();
        assert!(
            bpm_aus_frames(&rampe, 30.0).is_none(),
            "Rampe ist kein Takt"
        );
        // Leichte Drift auf einem Takt: der Takt bleibt trotzdem erkennbar.
        let mit_drift: Vec<f32> = sinus(120.0, 30.0, 12.0)
            .iter()
            .enumerate()
            .map(|(i, v)| v + 0.2 * i as f32 / 360.0)
            .collect();
        let bpm = bpm_aus_frames(&mit_drift, 30.0).expect("Takt unter der Drift");
        assert!((bpm - 120.0).abs() < 1.5, "erwartet 120, war {bpm}");
    }

    #[test]
    fn konfidenz_trennt_takt_von_rauschen() {
        let takt = sinus(120.0, 30.0, 12.0);
        let k_takt = konfidenz(&takt, 30.0, 120.0);
        let k_rauschen = konfidenz(&rauschen(360), 30.0, 120.0);
        assert!(k_takt > 0.5, "Takt muss überzeugen, war {k_takt}");
        assert!(
            k_rauschen < 0.5,
            "Rauschen darf nicht überzeugen, war {k_rauschen}"
        );
        // Ausserhalb des Bereichs oder ohne Reihe: 0.
        assert_eq!(konfidenz(&takt, 30.0, 1000.0), 0.0);
        assert_eq!(konfidenz(&[], 30.0, 120.0), 0.0);
        assert_eq!(konfidenz(&takt, 0.0, 120.0), 0.0);
    }

    #[test]
    fn dauer_text_zeigt_sekunden_oder_unbekannt() {
        // Punkt 9: die Brücke liefert Millisekunden, geloggt wird „12.3 s“.
        assert_eq!(dauer_text(Some(12.34)), "12.3 s");
        assert_eq!(dauer_text(Some(0.4)), "0.4 s");
        assert_eq!(dauer_text(Some(3725.5)), "3725.5 s");
        // Unbekannt ist alles, was keine brauchbare Dauer ist: fehlend,
        // Retriever-Nullwert, NaN/unendlich und negative Werte.
        assert_eq!(dauer_text(None), "unbekannt");
        assert_eq!(dauer_text(Some(0.0)), "unbekannt");
        assert_eq!(dauer_text(Some(0.04)), "unbekannt");
        assert_eq!(dauer_text(Some(-2.0)), "unbekannt");
        assert_eq!(dauer_text(Some(f32::NAN)), "unbekannt");
        assert_eq!(dauer_text(Some(f32::INFINITY)), "unbekannt");
    }

    /// Pulszug mit beliebiger (auch nicht ganzzahliger) Periode: je Schlag ein
    /// Plateau der Breite `breite` Frames, dazwischen 0; `rausch` legt
    /// gleichverteiltes Rauschen (±0,5·rausch) darüber.
    fn puls(bpm: f32, fps: f32, sekunden: f32, breite: usize, rausch: f32) -> Vec<f32> {
        let n = (fps * sekunden) as usize;
        let periode = 60.0 * fps / bpm;
        let roh = rauschen(n);
        let mut v = vec![0.0f32; n];
        let mut k = 0usize;
        loop {
            let start = (k as f32 * periode).round() as usize;
            if start >= n {
                break;
            }
            for i in start..(start + breite).min(n) {
                v[i] = 1.0;
            }
            k += 1;
        }
        for (i, wert) in v.iter_mut().enumerate() {
            *wert += rausch * roh[i];
        }
        v
    }

    #[test]
    fn puls_mit_krummer_periode_bleibt_im_takt() {
        // 128 BPM bei 20 fps sind 9,375 Frames je Schlag — kein ganzzahliger
        // Lag. Die Autokorrelation ist deshalb beim Dreifachen (Lag 28)
        // stärker als beim Fundament (Lag 9); früher kam 42,7 BPM heraus.
        let werte = puls(128.0, 20.0, 20.0, 2, 0.05);
        let bpm = bpm_aus_frames(&werte, 20.0).expect("128 BPM im Pulszug");
        assert!((bpm - 128.0).abs() < 2.0, "erwartet 128, war {bpm}");
    }

    #[test]
    fn puls_140_bei_20_fps_faellt_nicht_auf_halbzeit() {
        // 140 BPM liegen bei 20 fps bei 8,57 Frames; der stärkste ganzzahlige
        // Lag ist 17 (zwei Schläge) — das ist 70 BPM. Der halbe Wert darf
        // nicht gewinnen, wenn er nur vom Lag-Raster kommt.
        let werte = puls(140.0, 20.0, 20.0, 2, 0.1);
        let bpm = bpm_aus_frames(&werte, 20.0).expect("140 BPM im Pulszug");
        assert!((bpm - 140.0).abs() < 3.0, "erwartet 140, war {bpm}");
    }

    #[test]
    fn puls_bei_50_und_100_hz_abtastung() {
        // Dieselbe Helligkeitsfolge mit 50 und 100 Bildern je Sekunde
        // abgetastet (Kameras/Anzeigen liefern beides): das Tempo bleibt.
        for fps in [50.0f32, 100.0] {
            let werte = puls(120.0, fps, 12.0, 2, 0.05);
            let bpm = bpm_aus_frames(&werte, fps).expect("120 BPM bei {fps} fps");
            assert!(
                (bpm - 120.0).abs() < 1.5,
                "{fps} fps: erwartet 120, war {bpm}"
            );
        }
    }

    #[test]
    fn verrauschter_puls_wird_noch_erkannt() {
        // SNR unter 1: die Helligkeit schwankt stärker als der Schlag
        // (Rauschen ±0,2 um einen Puls der Höhe 1). Der Takt muss bleiben,
        // aber die Konfidenz darf kleiner sein als beim sauberen Puls.
        let werte = puls(120.0, 30.0, 12.0, 2, 0.4);
        let bpm = bpm_aus_frames(&werte, 30.0).expect("verrauschter 120er-Puls");
        assert!((bpm - 120.0).abs() < 2.0, "erwartet 120, war {bpm}");
        let k = konfidenz(&werte, 30.0, bpm);
        assert!(k > 0.3, "der Puls muss noch überzeugen, war {k}");
    }

    #[test]
    fn verrauschter_takt_bleibt_trotz_drift_erkannt() {
        // Ein echter Videotakt mit Belichtungsdrift und Rauschen: die
        // Ausgleichsgerade nimmt die Drift, der Puls bleibt.
        let mut werte = puls(100.0, 25.0, 16.0, 3, 0.3);
        for (i, v) in werte.iter_mut().enumerate() {
            *v += 0.8 * i as f32 / 400.0;
        }
        let bpm = bpm_aus_frames(&werte, 25.0).expect("Puls unter der Drift");
        assert!((bpm - 100.0).abs() < 2.5, "erwartet 100, war {bpm}");
    }

    #[test]
    fn betonter_takt_bleibt_bei_seinem_tempo() {
        // Jeder vierte Schlag ist stärker (Taktbetonung, wie eine Kick auf
        // der Eins). Der stärkste Autokorrelations-Peak liegt dadurch beim
        // Takt (vier Schläge), das Tempo muss trotzdem der Schlag bleiben.
        let mut werte = puls(160.0, 30.0, 20.0, 2, 0.05);
        let periode = 60.0 * 30.0 / 160.0;
        let mut k = 0usize;
        loop {
            let start = (k as f32 * periode).round() as usize;
            if start >= werte.len() {
                break;
            }
            if k % 4 == 0 {
                for i in start..(start + 2).min(werte.len()) {
                    werte[i] = 1.0;
                }
            } else {
                for i in start..(start + 2).min(werte.len()) {
                    werte[i] = werte[i].min(0.55);
                }
            }
            k += 1;
        }
        let bpm = bpm_aus_frames(&werte, 30.0).expect("betonter 160er-Takt");
        assert!((bpm - 160.0).abs() < 3.0, "erwartet 160, war {bpm}");
    }

    #[test]
    fn taktloses_rauschen_mit_drift_bleibt_none() {
        // Verrauschte, driftende Helligkeit ohne Puls (Gegenprobe zu den
        // Puls-Tests): kein lokales Maximum mit Prominenz.
        let roh = rauschen(600);
        let mut stand = 0.0f32;
        let reihe: Vec<f32> = roh
            .iter()
            .enumerate()
            .map(|(i, r)| {
                stand = 0.97 * stand + r;
                stand + 0.3 * i as f32 / 600.0
            })
            .collect();
        assert!(
            bpm_aus_frames(&reihe, 30.0).is_none(),
            "Rauschen mit Drift ist kein Takt"
        );
    }

    /// Netzflimmern mit fester Frequenz und beliebiger Phase (Cosinus): eine
    /// Sinus-Reihe mit Phase 0 trifft bei 50 Hz / 20 fps zufällig genau die
    /// Nullstellen und wäre konstant.
    fn flimmern(hz: f32, fps: f32, sekunden: f32) -> Vec<f32> {
        let n = (fps * sekunden) as usize;
        (0..n)
            .map(|i| {
                // In f64 rechnen: 100 Hz bei 20 fps sind exakt 5 Perioden je
                // Frame; in f32 bliebe ein Rundungsrest, der nach dem
                // Detrenden wie ein Takt aussähe.
                let t = i as f64 / fps as f64;
                let phase = 2.0 * std::f64::consts::PI * hz as f64 * t + 0.7;
                (0.5 + 0.5 * phase.cos()) as f32
            })
            .collect()
    }

    #[test]
    fn flimmern_bei_50_und_100_hz_liefert_nichts() {
        // Netzflimmern: 50 Hz bzw. 100 Hz Helligkeit, mit 20/30 fps
        // abgetastet. 50 Hz wird bei 20 fps zum Alias von 10 Hz (Periode 2
        // Frames) = 600 BPM — viermal über dem Bereich. Die Halbzeit-Faltung
        // darf daraus kein „150 BPM“ machen: das ist ein Vielfaches des
        // Alias, kein Takt. Echte 300 BPM (eine Oktave über dem Bereich)
        // werden weiterhin auf 150 gefaltet (siehe `bereich_grenzt…`).
        for (hz, fps) in [(50.0f32, 20.0f32), (100.0, 20.0), (50.0, 30.0), (100.0, 30.0)] {
            let werte = flimmern(hz, fps, 12.0);
            assert!(
                bpm_aus_frames(&werte, fps).is_none(),
                "{hz} Hz Flimmern bei {fps} fps ist kein Takt"
            );
        }
    }

    #[test]
    fn graumittel_ist_der_mittelwert() {
        // Zwei Pixel: schwarz und weiß → 127,5.
        let bild = vec![0u8, 0, 0, 255, 255, 255, 255, 255];
        assert!((graumittel(&bild) - 127.5).abs() < 1e-3);
        // Rot zählt wie Grün und Blau (Grauwert, keine Gewichtung).
        let rot = vec![255u8, 0, 0, 255];
        assert!((graumittel(&rot) - 85.0).abs() < 1e-3);
        assert_eq!(graumittel(&[]), 0.0);
        assert_eq!(graumittel(&[1, 2, 3]), 0.0, "halbes Pixel zählt nicht");
    }

    #[test]
    fn stroboskop_video_ergibt_das_tempo() {
        // Der ganze Weg der App: je Videobild ein RGBA-Puffer, daraus das
        // Graumittel, daraus die BPM. Das Bild flackert mit 120 BPM
        // (Stroboskop), aufgenommen mit 20 Bildern je Sekunde.
        let (fps, bpm) = (20.0f32, 120.0f32);
        let (breite, hoehe) = (32usize, 24usize);
        let mut reihe = Vec::new();
        for i in 0..(fps * 12.0) as usize {
            let t = i as f32 / fps;
            let helligkeit =
                (0.5 + 0.5 * (2.0 * std::f32::consts::PI * bpm / 60.0 * t).sin()) * 255.0;
            let pixel = helligkeit as u8;
            let bild: Vec<u8> = (0..breite * hoehe)
                .flat_map(|_| [pixel, pixel, pixel, 255])
                .collect();
            reihe.push(graumittel(&bild));
        }
        let erkannt = bpm_aus_frames(&reihe, fps).expect("Stroboskop bei 120 BPM");
        assert!(
            (erkannt - bpm).abs() < 1.5,
            "erwartet 120 BPM aus dem Video, war {erkannt}"
        );
        assert!(konfidenz(&reihe, fps, erkannt) > 0.5);
    }

    #[test]
    fn ausduennen_nimmt_jeden_n_ten_wert() {
        let werte = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        assert_eq!(ausduennen(&werte, 1), werte.to_vec(), "1 = alles behalten");
        assert_eq!(ausduennen(&werte, 2), vec![1.0, 3.0, 5.0]);
        assert_eq!(ausduennen(&werte, 3), vec![1.0, 4.0]);
        assert_eq!(ausduennen(&werte, 10), vec![1.0], "größer als die Reihe");
        assert_eq!(ausduennen(&werte, 0), werte.to_vec(), "0 zählt wie 1");
        assert!(ausduennen(&[], 4).is_empty());
    }

    #[test]
    fn klemme_faktor_bleibt_bei_zweierpotenzen() {
        assert_eq!(klemme_faktor(0), 1);
        assert_eq!(klemme_faktor(1), 1);
        assert_eq!(klemme_faktor(2), 2);
        assert_eq!(klemme_faktor(3), 2, "abrunden auf die Zweierpotenz");
        assert_eq!(klemme_faktor(4), 4);
        assert_eq!(klemme_faktor(100), 64);
        assert_eq!(klemme_faktor(128), 128);
        assert_eq!(klemme_faktor(1000), 128, "Obergrenze");
    }

    #[test]
    fn faktor_text_zeigt_bruch_und_frame() {
        assert_eq!(faktor_text(1), "1x (jeder Frame)");
        assert_eq!(faktor_text(2), "1/2 (jeder 2. Frame)");
        assert_eq!(faktor_text(8), "1/8 (jeder 8. Frame)");
        assert_eq!(faktor_text(128), "1/128 (jeder 128. Frame)");
        assert_eq!(faktor_text(3), faktor_text(2), "wird geklemmt");
        assert_eq!(faktor_kuerzel(1), "1x");
        assert_eq!(faktor_kuerzel(8), "1/8");
        assert_eq!(faktor_kuerzel(1000), "1/128", "wird geklemmt");
    }

    #[test]
    fn faktoren_decken_die_genres() {
        // av4-Tabelle (2026-10-19): bei der typischen Ausgabe um 200 BPM
        // (früher das Rand-Artefakt) treffen genau diese Brüche die
        // Genre-Bänder; ×2/×3 helfen, wenn die Auswertung tief faltet
        // (60 → 120/180). ×1/2, ×1/3, ×1/4 fehlen bewusst: sie bräuchten
        // 240–720 BPM Eingabe, die App gibt höchstens 220 aus.
        let liste = faktoren();
        assert_eq!((liste[0].0, liste[0].1), (1, 1), "×1 steht vorn");
        assert_eq!(faktor_bruchteil(1, 1), 1.0);
        assert_eq!(faktor_bruchteil(5, 8), 0.625);
        assert!((faktor_bruchteil(2, 3) - 2.0 / 3.0).abs() < 1e-6);
        assert!((faktor_bruchteil(5, 6) - 5.0 / 6.0).abs() < 1e-6);
        assert_eq!(faktor_bruchteil(7, 8), 0.875);
        assert_eq!(faktor_bruchteil(2, 1), 2.0);
        assert_eq!(faktor_bruchteil(3, 1), 3.0);
        assert_eq!(faktor_bruchteil(5, 0), 5.0, "Nenner 0 zählt wie 1");
        // Bei 200 BPM Ausgabe landet je ein Bruch in jedem Genre-Band.
        let bei_200: Vec<f32> = liste
            .iter()
            .map(|(z, n, _)| 200.0 * faktor_bruchteil(*z, *n))
            .collect();
        assert!(bei_200.contains(&125.0), "×5/8 → House/Groove");
        assert!(
            bei_200.iter().any(|b| (160.0..=175.0).contains(b)),
            "×5/6 → Breakbeat"
        );
        assert!(
            bei_200.iter().any(|b| (170.0..=180.0).contains(b)),
            "×7/8 → Drum and Bass"
        );
        // Jeder Knopf hat einen Hinweis, und kein Bruch ist unbrauchbar.
        assert!(liste.len() <= 7, "die Zeile im Bild-Tab muss passen");
        for (z, n, hinweis) in liste {
            assert!(!hinweis.is_empty(), "Genre-Hinweis fehlt für {z}/{n}");
            let f = faktor_bruchteil(*z, *n);
            assert!(f > 0.0 && f.is_finite(), "Faktor {z}/{n} ist unbrauchbar");
        }
    }

    #[test]
    fn faktor_klemmt_auf_den_bereich() {
        // ×2/×3 für tiefe Ausgaben: 60 → 120 (Techno) bzw. 180 (DnB).
        assert!((tempo_mit_faktor(60.0, 2, 1) - 120.0).abs() < 1e-3);
        assert!((tempo_mit_faktor(60.0, 3, 1) - 180.0).abs() < 1e-3);
        // Das Ergebnis bleibt im Suchbereich 40…220 BPM.
        assert_eq!(tempo_mit_faktor(200.0, 3, 1), MAX_BPM);
        assert_eq!(tempo_mit_faktor(50.0, 5, 8), MIN_BPM);
        assert_eq!(tempo_mit_faktor(200.0, 1, 1), 200.0);
        // ×5/8 aus der av4-Tabelle: 199,6 → 124,75 (House/Groove).
        assert!((tempo_mit_faktor(199.6, 5, 8) - 124.75).abs() < 1e-3);
    }

    #[test]
    fn ausduennen_holt_schnelles_muster_ins_tempo() {
        // Nutzermessung (2026-10-19): die Muster im Kamerabild sind viel zu
        // schnell — eine Periode von 5 ms sind 200 Hz = 12 000 BPM. Als
        // Zeitlupe wird nur jeder 8. Frame ausgewertet: die Abtastrate fällt
        // von 30 auf 3,75 fps, und 200 Hz liegen 1,25 Hz neben dem 53.
        // Vielfachen (53 × 3,75 = 198,75 Hz) — als Alias erscheinen
        // 1,25 Hz = 75 BPM im Genre-Bereich.
        let werte = sinus(12_000.0, 30.0, 12.0); // 200 Hz bei 30 fps
        let geduennt = ausduennen(&werte, 8);
        assert_eq!(geduennt.len(), 45, "12 s × 30 fps / 8");
        let bpm = bpm_aus_frames(&geduennt, 30.0 / 8.0).expect("Alias muss im Bereich landen");
        assert!(
            (MIN_BPM..=MAX_BPM).contains(&bpm),
            "plausibles Tempo erwartet, war {bpm}"
        );
        assert!((bpm - 75.0).abs() < 2.0, "erwartet 75 BPM, war {bpm}");

        // Zu grob geht es nicht: bei Faktor 16 sinkt die Abtastrate auf
        // 1,875 fps. Dort hat der Bereich 40…220 BPM (Lags 0,5…2,8) keinen
        // ganzzahligen Spielraum mehr — lieber nichts als geraten.
        assert!(bpm_aus_frames(&ausduennen(&werte, 16), 30.0 / 16.0).is_none());
    }

    #[test]
    fn bereich_grenzt_die_suche_ein() {
        // Ein 300-BPM-Klickzug (Periode 0,2 s) fällt bei einem Bereich bis
        // 220 BPM auf 150 (halb) — 150 liegt näher an der Mitte (130) als 75.
        let werte = klicks(300.0, 60.0, 12.0);
        let bpm = bpm_aus_reihe(&werte, 60.0, 40.0, 220.0).expect("300 BPM im Bereich");
        assert!((bpm - 150.0).abs() < 2.0, "erwartet 150, war {bpm}");
        // Ein enger Bereich, in dem gar keine Periode liegt (Lags 28…32, der
        // Zug hat seine Vielfachen bei 12/24/36): lieber nichts als geraten.
        assert!(bpm_aus_reihe(&werte, 60.0, 110.0, 130.0).is_none());
    }
}
