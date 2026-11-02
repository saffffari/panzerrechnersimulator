# The repair manual, rebuilt as a Fibel — 2 November 2026

The expedition's physical field guide (`src/repair_manual.rs`, 16 spreads of a 0.34 x 0.24 m
book) was laid out like a pocket notebook: portrait pages of three sentences and a schematic
box drawing, held at arm's length, so the diagnosis reads but the detail does not. The owner's
direction: a repair manual needs higher detail, more of the screen, a loupe to drag over the
page, drawings from the Tiger CAD, and the typography and graphic design of the real thing.

## The real thing

D 656/27 *Tigerfibel* (1 August 1943, Generalinspekteur der Panzertruppen; written by Josef von
Glatter-Götz, drawn by Gessinger and Wagner) is the model. What its pages establish, from the
public-domain scans on Wikimedia Commons (`D656-27 Tigerfibel Seite 15 (Tanken)`,
`Inhaltsverzeichnis Seite 9`, `Seite 91 (Panzerklau)`) and the archive.org copy
(`der-generalinspekteur-der-panzertruppen-die-tigerfibel-1943`):

- **Format**: A5 landscape pages (the scans are 655 x 475 px, 1.38:1), bound on the short side,
  a page number at the outer bottom corner, wide outer margins.
- **Two colours**: black and one warm red on cream. Red carries the lead words ("Darum:",
  "Sonst", "Achtung!", "Männer vom Tiger!") and the fills in the drawings (fuel cans, the road,
  a part to notice); black carries the text and the line art.
- **Type**: a light grotesque, set small; section titles letter-spaced in capitals across the
  page ("I N H A L T S V E R Z E I C H N I S"); chapter titles sometimes in a brush script
  ("Panzerklau"); subheads in red italic; a contents page with leader dots.
- **Drawings**: line art with hatching and spot red, often an exploded or cut-away machine with
  a figure for scale; cartoons with the same pen; diagrams of flow (fuel to Berlin) and of
  procedure (six steps numbered).
- **Voice**: rhymes and jokes to make the facts stick. The game keeps its English, its
  diagnosis tree and its wordless-play rule; it borrows the design, not the verse.

## What changes

1. **Format.** Landscape A5 pages, 210 x 148 mm, bound on the short edge: the open book is
   0.42 m wide. Held like the map (`reading_fov`), the spread fills most of the frame width;
   with the loupe over it the print is read at its real size.
2. **Ink.** Page albedo at 1536 x 1086 (A5 landscape at 185 dpi; was 768 x 1024 portrait);
   four retained surfaces as before. The page layout engine (`draw_page`) sets: a letter-spaced
   capitals title band with a rule; a red lead word per paragraph; two text columns on text
   pages; a drawing plate dominating a facing page; a contents spread with leader dots; page
   numbers at the outer corners; the diagnosis choices as red arrows at the foot.
3. **Type.** Two more faces in the ink atlas: a grotesque regular and black, loaded at run time
   from the asset root (`assets/fonts/NHaasGrotesk-Md.otf`, `-Blk.otf`, the owner's licensed
   faces; never committed) with the bundled `VT323`/`PressStart2P` as the fallback when they are
   absent, so a clone without the assets still prints. The map and the strip keep the pixel faces.
4. **Drawings.** Plates derived from the owner's Tiger CAD (`_Reference_CAD_Tigers`, a 1/35
   kit CAD scaled 35x; see `tools/bake_manual_plates.py`): FreeCAD TechDraw hidden-line
   projections of the engine system, the base unit (hull, running gear, final drives) and the
   assemblies the chapters need (track and link, drive train, running gear, recovery rigging),
   rasterised by `rsvg-convert` to black line art with a red spot layer for the part the page
   is about, stored under `assets/manual/plates/` (not committed) with a bake report. The
   schematic vocabulary stays as the fallback when a plate is missing.
5. **The loupe.** A hand lens the reader drags over the page: a second camera at the lens
   (the mechanism of the gunner's optic composite, `render_optic_composite`), a circular
   inset at about 3x, following the pointer. It is a lens, not a zoom of the texture: the page
   is re-rendered through it under the same light.
6. **Captures and gates.** `repair-manual-capture` re-run for every spread and the loupe; the
   eight manual tests kept; a plate-presence test that fails loudly when the asset root has the
   plates but a page names one that is not there.

## Order of work

Format and ink first (the book reads at once), the faces second, the plates third (the bake
tool can run while the pages are being set), the loupe last.
