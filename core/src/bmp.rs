//! Kleiner BMP-Schreiber (24 Bit) für den Bild-Snapshot.
//!
//! Der Desktop-rrn5 speichert mit der `image`-Kiste; für Android genügt ein
//! eigener Schreiber ohne neue Abhängigkeit: BMP legt die Zeilen von **unten
//! nach oben** ab, jedes Pixel als BGR, und füllt jede Zeile auf 4 Byte auf.

/// 24-Bit-BMP aus einem RGBA-Puffer (Breite × Höhe, 4 Byte je Pixel).
///
/// Zu kurze Puffer werden am Ende mit Schwarz aufgefüllt; zu lange ignoriert.
pub fn bmp_aus_rgba(breite: usize, hoehe: usize, rgba: &[u8]) -> Vec<u8> {
    let breite = breite.max(1);
    let hoehe = hoehe.max(1);
    let zeilen_bytes = ((breite * 3 + 3) / 4) * 4;
    let daten = zeilen_bytes * hoehe;
    let dateigroesse = 54 + daten;

    let mut out = Vec::with_capacity(dateigroesse);
    // BITMAPFILEHEADER
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(dateigroesse as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    // BITMAPINFOHEADER
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(breite as i32).to_le_bytes());
    out.extend_from_slice(&(hoehe as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(daten as u32).to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    // Pixel: unterste Zeile zuerst, BGR, rechts mit Nullbytes aufgefüllt.
    for zeile in (0..hoehe).rev() {
        let mut geschrieben = 0usize;
        for spalte in 0..breite {
            let idx = (zeile * breite + spalte) * 4;
            let (r, g, b) = (
                rgba.get(idx).copied().unwrap_or(0),
                rgba.get(idx + 1).copied().unwrap_or(0),
                rgba.get(idx + 2).copied().unwrap_or(0),
            );
            out.push(b);
            out.push(g);
            out.push(r);
            geschrieben += 3;
        }
        while geschrieben < zeilen_bytes {
            out.push(0);
            geschrieben += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_le(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn i32_le(bytes: &[u8], offset: usize) -> i32 {
        i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn kopf_ist_ein_bmp_mit_24_bit() {
        let bmp = bmp_aus_rgba(2, 2, &[0u8; 16]);
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(u32_le(&bmp, 2) as usize, bmp.len(), "Dateigröße im Kopf");
        assert_eq!(
            u32_le(&bmp, 10) as usize,
            54,
            "Pixel beginnen nach dem Kopf"
        );
        assert_eq!(u32_le(&bmp, 14) as usize, 40, "BITMAPINFOHEADER");
        assert_eq!(i32_le(&bmp, 18), 2, "Breite");
        assert_eq!(i32_le(&bmp, 22), 2, "Höhe");
        assert_eq!(u32_le(&bmp, 28) & 0xffff, 24, "24 Bit je Pixel");
        assert_eq!(u32_le(&bmp, 30), 0, "keine Kompression");
        assert_eq!(u32_le(&bmp, 34) as usize, bmp.len() - 54, "Bilddaten");
    }

    #[test]
    fn zeilen_werden_auf_vier_byte_aufgefuellt() {
        // 1 Pixel breit: 3 Byte Daten + 1 Byte Füllung je Zeile.
        let bmp = bmp_aus_rgba(1, 2, &[10, 20, 30, 255, 40, 50, 60, 255]);
        assert_eq!(bmp.len(), 54 + 2 * 4);
        // Unten zuerst: die erste Datenzeile ist Pixel (0,1).
        assert_eq!(&bmp[54..57], &[60, 50, 40], "BGR der unteren Zeile");
        assert_eq!(bmp[57], 0, "Füllbyte");
        assert_eq!(&bmp[58..61], &[30, 20, 10], "BGR der oberen Zeile");
    }

    #[test]
    fn farben_kommen_bgr_und_unten_zuerst() {
        // 2x2: oben links rot, oben rechts grün, unten links blau, unten rechts weiß.
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 255, // Zeile 0 (oben)
            0, 0, 255, 255, 255, 255, 255, 255, // Zeile 1 (unten)
        ];
        let bmp = bmp_aus_rgba(2, 2, &rgba);
        assert_eq!(
            &bmp[54..60],
            &[255, 0, 0, 255, 255, 255],
            "untere Zeile BGR"
        );
        assert_eq!(&bmp[62..68], &[0, 0, 255, 0, 255, 0], "obere Zeile BGR");
    }

    #[test]
    fn zu_kurzer_puffer_bleibt_schwarz() {
        let bmp = bmp_aus_rgba(2, 1, &[1, 2, 3, 255]);
        assert_eq!(bmp.len(), 54 + 8, "zweite Zeile fehlt -> aufgefüllt");
        assert_eq!(&bmp[54..57], &[3, 2, 1], "erstes Pixel BGR");
        assert_eq!(&bmp[57..60], &[0, 0, 0], "fehlendes Pixel wird schwarz");
    }
}
