//! YUV_420_888 (Kamera-Ausgabe von Android) nach RGBA.
//!
//! Android liefert die drei Planes mit eigenem Row- und Pixel-Stride; bei
//! vielen Geräten sind U und V verschränkt (PixelStride 2) und die Zeilen mit
//! Füllbytes am Ende (RowStride > Breite). Die Konvertierung nach BT.601 mit
//! begrenztem Wertebereich (16–235) ist die übliche Kamera-Vorgabe.

/// Eine Plane: Pixel mit eigenem Zeilen- und Pixelschritt.
#[derive(Clone, Copy, Debug)]
pub struct Plane<'a> {
    pub data: &'a [u8],
    pub row_stride: usize,
    pub pixel_stride: usize,
}

impl<'a> Plane<'a> {
    fn pixel(&self, x: usize, y: usize) -> u8 {
        let idx = y * self.row_stride + x * self.pixel_stride;
        *self.data.get(idx).unwrap_or(&0)
    }
}

/// Schreibt `width * height` RGBA-Bytes nach `out` (wird geleert und gefüllt).
pub fn yuv420_to_rgba(
    y: Plane,
    u: Plane,
    v: Plane,
    width: usize,
    height: usize,
    out: &mut Vec<u8>,
) {
    out.clear();
    out.reserve(width * height * 4);
    for row in 0..height {
        for col in 0..width {
            let yy = y.pixel(col, row) as i32;
            let uu = u.pixel(col / 2, row / 2) as i32 - 128;
            let vv = v.pixel(col / 2, row / 2) as i32 - 128;
            let c = (yy - 16).max(0) * 298;
            let r = ((c + 409 * vv + 128) >> 8).clamp(0, 255) as u8;
            let g = ((c - 100 * vv - 208 * uu + 128) >> 8).clamp(0, 255) as u8;
            let b = ((c + 516 * uu + 128) >> 8).clamp(0, 255) as u8;
            out.push(r);
            out.push(g);
            out.push(b);
            out.push(255);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(data: &[u8], row_stride: usize) -> Plane<'_> {
        Plane {
            data,
            row_stride,
            pixel_stride: 1,
        }
    }

    /// Y, U, V als eine Plane je Wert (kein Padding, PixelStride 1).
    fn flaeche(y: u8, u: u8, v: u8, w: usize, h: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let yp = vec![y; w * h];
        let cp = vec![0u8; w.div_ceil(2) * h.div_ceil(2)];
        let mut up = cp.clone();
        let mut vp = cp;
        for p in up.iter_mut() {
            *p = u;
        }
        for p in vp.iter_mut() {
            *p = v;
        }
        yuv420_to_rgba(
            plane(&yp, w),
            plane(&up, w.div_ceil(2)),
            plane(&vp, w.div_ceil(2)),
            w,
            h,
            &mut out,
        );
        out
    }

    #[test]
    fn schwarz_und_weiss_treffen_die_endpunkte() {
        // BT.601 mit begrenztem Bereich: 16 ist Schwarz, 235 ist Weiss.
        let schwarz = flaeche(16, 128, 128, 4, 2);
        assert_eq!(&schwarz[0..4], &[0, 0, 0, 255]);
        let weiss = flaeche(235, 128, 128, 4, 2);
        assert_eq!(&weiss[0..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn farben_treffen_die_erwarteten_werte() {
        // Aus der Formel gerechnet, damit die Richtung stimmt:
        // grosses V -> rot, grosses U -> blau.
        let rot = flaeche(82, 90, 240, 2, 2);
        assert!(rot[0] > 200, "Rotkanal, war {}", rot[0]);
        assert!(rot[1] < 80, "Gruenkanal, war {}", rot[1]);
        let blau = flaeche(41, 240, 110, 2, 2);
        assert!(blau[2] > 200, "Blaukanal, war {}", blau[2]);
    }

    #[test]
    fn rgba_hat_vier_kanaele_und_die_richtige_laenge() {
        let bild = flaeche(128, 128, 128, 5, 3);
        assert_eq!(bild.len(), 5 * 3 * 4);
        assert!(bild.chunks(4).all(|p| p[3] == 255), "Alpha ist deckend");
    }

    #[test]
    fn zeilenfuellung_wird_uebersprungen() {
        // Y mit 6 Byte Zeilenfuellung bei 4 Pixeln Breite: nur die ersten
        // vier Bytes je Zeile duerfen zaehlen.
        let y = [
            200, 200, 200, 200, 9, 9, // Zeile 0, Fuellung
            60, 60, 60, 60, 9, 9, // Zeile 1, Fuellung
        ];
        let u = [128u8; 2];
        let v = [128u8; 2];
        let mut out = Vec::new();
        yuv420_to_rgba(plane(&y, 6), plane(&u, 2), plane(&v, 2), 4, 2, &mut out);
        let erste = &out[0..4];
        let zweite = &out[4 * 4..4 * 4 + 4];
        assert!(erste[0] > 200, "Zeile 0 hell, war {}", erste[0]);
        assert!(zweite[0] < 80, "Zeile 1 dunkel, war {}", zweite[0]);
    }

    #[test]
    fn verschraenkte_farbebenen_werden_gelesen() {
        // PixelStride 2 wie bei halbplanarem YUV: die U-Werte stehen an
        // geraden Stellen, dazwischen liegt V.
        let y = [128u8; 4];
        let uv = [128u8, 240, 128, 240]; // U=128, V=240 (zweimal)
        let u = Plane {
            data: &uv,
            row_stride: 4,
            pixel_stride: 2,
        };
        let v = Plane {
            data: &uv[1..],
            row_stride: 4,
            pixel_stride: 2,
        };
        let mut out = Vec::new();
        yuv420_to_rgba(plane(&y, 2), u, v, 2, 2, &mut out);
        assert!(out[0] > 120, "rotstichig, war {}", out[0]);
    }
}
