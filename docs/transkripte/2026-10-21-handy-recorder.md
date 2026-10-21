# Handy-Audiorecorder — Transkripte (2026-10-21)

Aufgenommen mit dem Pixel-Recorder (`/sdcard/Recordings`), übertragen per USB
(`adb pull` nach `~/Recordings-Handy/`), transkribiert mit **whisper.cpp
1.8.7** über `tools/transkribieren.sh`.

Zwei Läufe: zuerst `ggml-small` (roh, `/tmp/opencode/transkripte/`), dann
`ggml-medium` (deutlich besser, `/tmp/opencode/transkripte_medium/` — unten
zitiert). Die Erkennung bleibt fehleranfällig; unten steht jeweils die
**Lesart**, die zum Projekt passt.

| Aufnahme | Länge | Transkript (medium) |
|---|---:|---|
| 14. Okt. 17:26 | 11 s | „Das Gyroskop im Handy nutzen, um bei ausgewähltem Bild die Tonhöhe zu verändern.“ |
| 16. Okt. 20:46 | 29 s | „Beim Filmen des Herbstlaufs auf dem Weg soll die Objekterkennung beim Scannen passende Abstände von Blättern finden und sie in einen Rhythmus nach dem gewählten Genre einfügen, dann soll das Scannen sehen.“ |
| 17. Okt. 18:27 | 13 s | „Ich zeig Ränder. Die Stelle oder den Bereich kann ich verschieben. Er wird mit anderen Farben dargestellt.“ |
| 18. Okt. 17:06 | 18 s | „Aus den Melodien der Thai-Massage-Musik, die Akkorde als Mini-Presets, Speicher zum Auswählen für Echtzeitereinladungen.“ |
| 20. Okt. 05:02 | 27 s | „Das IT-Textfeld noch mit einfügen und da rein gehören, welche Pattern, an welchem Tag, zu welchem Bild aufgenommen wurde. Und Platz, dass ich noch Notizen hinzufügen kann.“ |
| 20. Okt. 05:03 | 6 s | „Treckeransicht als Standard.“ |
| 20. Okt. 05:05 | 92 s | „Bei Laub die Abstände nicht zwischen den genauen Farbwerten, sondern zwischen den Farbmischwerten berechnen. Damit ist mehr Kompatibilität zu verschiedenen Genres möglich. … eine größere Menge an Auswahlmöglichkeiten, die wiederum gefiltert werden muss oder Selektionskriterien gebraucht, über die ich mir noch nicht im Klaren bin.“ |
| 20. Okt. 05:07 | 52 s | „Noch zu dem ‚Lauf‘-Tab für die Bestimmung der Abstände mit Farbmischwerten. Suche nach Beschreibungen für diese neu hinzugekommenen Diversitäten in den Subgruppen und Gruppen der Taktabstände.“ / „Vielen Dank für's Zuhören!“ |
| 20. Okt. 05:39 | 19 s | „Das kann man in einer Übersicht darstellen, wenn man das Programm bedient, welche Teile vom Codegrad genutzt werden und gegebenenfalls auch die Pipeline zwischen den einzelnen Dateien farbig darstellen.“ |

## Lesart (was gemeint ist)

1. **Gyroskop → Tonhöhe** (14.10., **zurückgestellt**): Bei ausgewähltem Bild
   die Tonhöhe über die Handy-Neigung verändern.
2. **Filmen → Scan → Rhythmus** (16.10.): Beim Filmen des Laubwegs die
   Blattabstände im Scan **sichtbar** erkennen und nach Genre in einen Rhythmus
   schreiben (Video→Raster, Marker, Genre-Rhythmus sind umgesetzt).
3. **Ränder/Scan-Bereich verschieben** (17.10., **geklärt am 21.10.**):
   „Verschieben-Button für Linie oder Bereich, also Scan-Bereich“ — im
   Bild-Tab die Scan-Linie bzw. den scan-Bereich per Knopf verschieben; die
   Ränder werden in anderen Farben dargestellt. Plan:
   `docs/plans/2026-10-21-scan-verschieben.md`.
4. **Akkord-Mini-Presets** (18.10.): Akkorde als Presets speichern und für die
   Echtzeit-Noteneingabe auswählen (umgesetzt von Orchestrator A).
5. **.it-Metadaten** (20.10. 05:02): Im IT-Textfeld festhalten, **welches
   Pattern an welchem Tag zu welchem Bild** aufgenommen wurde, plus **Notizen**
   (Orchestrator B).
6. **Tracker als Standardansicht** (20.10. 05:03, umgesetzt).
7. **Farbmischwerte statt exakter Farben** (20.10. 05:05, umgesetzt) + mehr
   Selektions-/Filterkriterien (offen, noch unklar welche).
8. **Form-Mischwerte + Beschreibungen** (20.10. 05:07, umgesetzt) +
   Bezeichnungen für Untergruppen/Gruppen der Taktabstände.
9. **Code-/Pipeline-Übersicht** (20.10. 05:39): Übersicht, welche Codeteile
   benutzt werden, Pipeline zwischen Dateien farbig (Orchestrator B).
