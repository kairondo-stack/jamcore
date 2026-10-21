# Handy-Audiorecorder — Transkripte (2026-10-21)

Aufgenommen mit dem Pixel-Recorder (`/sdcard/Recordings`), übertragen per USB
(`adb pull` nach `~/Recordings-Handy/`), transkribiert mit **whisper.cpp
1.8.7** (`ggml-small`, Deutsch) über `tools/transkribieren.sh`.
Roh-Transkripte: `/tmp/opencode/transkripte/*.txt` (eine .txt je Aufnahme,
mit Zeitmarken). Die Erkennung ist nicht fehlerfrei — unten steht jeweils die
**Lesart**, die zum Projekt passt.

| Aufnahme | Länge | Roh-Transkript (wörtlich) |
|---|---:|---|
| 14. Okt. 17:26 | 11 s | „Das Gyroskop im Handy nutzen, um bei ausgewähltem Bild die Tonhöhe zu verändern.“ |
| 16. Okt. 20:46 | 29 s | „Beim Filmen des Herbstlaufs auf dem Weg füllt die Erkennung beim Scan passende Abstände von Blättern finden und sie in einen Rhythmus nach dem gewählten Genre einfügen. Man soll das im Scan sehen.“ |
| 17. Okt. 18:27 | 13 s | „Ich zerdrenne nun, die Stelle oder den Bereich kann ich verschieben.“ / „Ich würde mit anderen Fragen da anstellen.“ / „Nee, ich zeig dir das.“ |
| 18. Okt. 17:06 | 18 s | „Aus dem Halben der Thai-Massage-Musik, die Akkord als Mini-Presets speichert zum Auswählen für Echtzeitereinnotten.“ |
| 20. Okt. 05:02 | 27 s | „Das IT-Text fällt noch mit einfügender und da rein gehören, welche Pattern, an welchem Tag, zu welchem Bild aufgenommen wurde und Platz, dass ich noch Notizen hinzufügen kann.“ |
| 20. Okt. 05:03 | 6 s | „Trecker an Sicht als Standard.“ |
| 20. Okt. 05:05 | 92 s | „Die Abstände nicht zwischen den genauen Farbwerten, sondern zwischen den Farbmischwerten. Damit ist mehr Kompatibilität zu verschiedenen Jongers möglich.“ / „Eine größere Menge an Ausperrmöglichkeiten, die wiederum gefiltert werden muss. Über die ich mir noch nicht im Klamm bin.“ |
| 20. Okt. 05:07 | 52 s | „Noch zu dem Laubtab für die Bestimmung der Abstände mit Form-Mischwerten. Sucher nach Beschreibungen für diese neu hinzugekommenen Diversitäten in den Subgruppen und Gruppen der Taktabstände.“ / „[Aufregende Musik]“ |
| 20. Okt. 05:39 | 19 s | „Kann man in einer Übersicht darstellen, wenn man das Programm bedient, welche Teile vom Codegrad genutzt werden. Und gegebenenfalls auch die Pipeline zwischen den einzelnen Dateien farbig darstellen.“ |

## Lesart (was gemeint ist)

1. **Gyroskop → Tonhöhe** (14.10.): Bei ausgewähltem Bild die Tonhöhe über die
   Handy-Neigung (Gyroskop) verändern — live, statt Regler.
2. **Filmen → Scan → Rhythmus** (16.10.): Beim Filmen des Laubwegs sollen die
   Blattabstände im Scan **sichtbar** erkannt und nach dem gewählten Genre in
   einen Rhythmus geschrieben werden. (Teilweise umgesetzt: Video → Raster;
   „im Scan sehen“ = Marker/Abstände im Bild — vorhanden; Genre-Rhythmus aus
   Video — vorhanden.)
3. **Bereich verschieben** (17.10.): unklar („die Stelle oder den Bereich kann
   ich verschieben“) — vermutlich Scan-/Slicer-Bereich per Finger verschieben;
   Rückfrage nötig.
4. **Akkord-Mini-Presets** (18.10.): Akkorde als kleine Presets speichern und
   für die Echtzeit-Noteneingabe auswählen.
5. **.it-Metadaten** (20.10. 05:02): Im IT-Text/den Metadaten festhalten,
   **welches Pattern an welchem Tag zu welchem Bild** aufgenommen wurde, plus
   **Platz für Notizen**.
6. **Tracker als Standardansicht** (20.10. 05:03): Rhythmus-Tab startet in der
   Tracker-Ansicht statt im Raster.
7. **Farbmischwerte statt exakter Farben** (20.10. 05:05): Abstände über
   **Farbmischwerte** (z. B. Rot/Gelb/Braun/Grün als Mischungen) statt exakter
   RGB-Werte bilden → kompatibler zu mehreren Genres; dazu mehr Filter-/
   Ausschlussmöglichkeiten (noch unklar, welche).
8. **Form-Mischwerte + Beschreibungen** (20.10. 05:07): Laub-Tab: Abstände auch
   über **Form-Mischwerte**; Bezeichnungen für die neuen Diversitäten in den
   **Untergruppen/Gruppen der Taktabstände** finden.
9. **Code-/Pipeline-Übersicht** (20.10. 05:39): Eine Ansicht, die beim Bedienen
   zeigt, **welche Codeteile benutzt werden**, und die **Pipeline zwischen den
   Dateien farbig** darstellt.
