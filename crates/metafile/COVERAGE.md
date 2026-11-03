# Reader coverage audit — ticket #92

Compared first-party source snapshots listed in README.md, not app marketing
claims. VectorCraft columns describe **import only**, never its writer. `Y`
means playback/handling, not pixel-exact GDI fidelity; `—` means skipped;
`slot` means only a non-drawing object placeholder. `partial` is qualified below.
Headers/EOF are handled outside record dispatch. EMF+ comments likewise live in
the EMF traversal. VectorCraft's WMF window records are handled in its outer
loop, only for placeable mapping. No WordCraft record dispatch was dropped.

## EMF records (decimal IDs)

| Record | VectorCraft import | WordCraft | DeckCraft | Shared |
|---|---|---|---|---|
| 1 HEADER | Y | Y | Y | Y |
| 2 POLYBEZIER | Y | Y | Y | Y |
| 3 POLYGON | Y | Y | Y | Y |
| 4 POLYLINE | Y | Y | Y | Y |
| 5 POLYBEZIERTO | Y | Y | Y | Y |
| 6 POLYLINETO | Y | Y | Y | Y |
| 7 POLYPOLYLINE | Y | Y | Y | Y |
| 8 POLYPOLYGON | Y | Y | Y | Y |
| 9 SETWINDOWEXTEX | Y | Y | Y | Y |
| 10 SETWINDOWORGEX | Y | Y | Y | Y |
| 11 SETVIEWPORTEXTEX | Y | Y | Y | Y |
| 12 SETVIEWPORTORGEX | Y | Y | Y | Y |
| 14 EOF | Y | Y | Y | Y |
| 17 SETMAPMODE | Y | Y | Y | Y |
| 18 SETBKMODE | — | — | Y | Y |
| 19 SETPOLYFILLMODE | Y | Y | Y | Y |
| 20 SETROP2 | — | Y | Y | Y |
| 22 SETTEXTALIGN | Y | Y | Y | Y |
| 24 SETTEXTCOLOR | Y | Y | Y | Y |
| 25 SETBKCOLOR | — | — | Y | Y |
| 27 MOVETOEX | Y | Y | Y | Y |
| 29 EXCLUDECLIPRECT | partial | — | Y | Y |
| 30 INTERSECTCLIPRECT | Y | — | Y | Y |
| 31 SCALEVIEWPORTEXTEX | Y | — | — | Y |
| 32 SCALEWINDOWEXTEX | Y | — | — | Y |
| 33 SAVEDC | Y | Y | Y | Y |
| 34 RESTOREDC | Y | Y | Y | Y |
| 35 SETWORLDTRANSFORM | Y | Y | Y | Y |
| 36 MODIFYWORLDTRANSFORM | Y | Y | Y | Y |
| 37 SELECTOBJECT | Y | Y | Y | Y |
| 38 CREATEPEN | Y | Y | Y | Y |
| 39 CREATEBRUSHINDIRECT | solid/none | solid/none | Y | Y |
| 40 DELETEOBJECT | Y | Y | Y | Y |
| 42 ELLIPSE | Y | Y | Y | Y |
| 43 RECTANGLE | Y | Y | Y | Y |
| 44 ROUNDRECT | Y | Y | Y | Y |
| 45 ARC | Y | Y | Y | Y |
| 46 CHORD | Y | Y | Y | Y |
| 47 PIE | Y | Y | Y | Y |
| 49 CREATEPALETTE | slot | — | — | slot |
| 54 LINETO | Y | Y | Y | Y |
| 57 SETARCDIRECTION | Y | — | — | Y |
| 58 SETMITERLIMIT | Y | — | — | — |
| 59 BEGINPATH | Y | Y | Y | Y |
| 60 ENDPATH | Y | Y | Y | Y |
| 61 CLOSEFIGURE | Y | Y | Y | Y |
| 62 FILLPATH | Y | Y | Y | Y |
| 63 STROKEANDFILLPATH | Y | Y | Y | Y |
| 64 STROKEPATH | Y | Y | Y | Y |
| 67 SELECTCLIPPATH | Y | — | — | — |
| 68 ABORTPATH | Y | — | — | Y |
| 70 COMMENT | detect only | — | EMF+ playback | EMF+ playback |
| 75 EXTSELECTCLIPRGN | Y | — | Y | Y |
| 76 BITBLT | Y | Y | Y | Y |
| 77 STRETCHBLT | Y | Y | Y | Y |
| 80 SETDIBITSTODEVICE | — | Y | Y | Y |
| 81 STRETCHDIBITS | Y | Y | Y | Y |
| 82 EXTCREATEFONTINDIRECTW | Y | Y | Y | Y |
| 83 EXTTEXTOUTA | Y | — | Y | Y |
| 84 EXTTEXTOUTW | Y | Y | Y | Y |
| 85 POLYBEZIER16 | Y | Y | Y | Y |
| 86 POLYGON16 | Y | Y | Y | Y |
| 87 POLYLINE16 | Y | Y | Y | Y |
| 88 POLYBEZIERTO16 | Y | Y | Y | Y |
| 89 POLYLINETO16 | Y | Y | Y | Y |
| 90 POLYPOLYLINE16 | Y | Y | Y | Y |
| 91 POLYPOLYGON16 | Y | Y | Y | Y |
| 93 CREATEMONOBRUSH | slot | — | Y | Y |
| 94 CREATEDIBPATTERNBRUSHPT | slot | — | Y | Y |
| 95 EXTCREATEPEN | Y | Y (style gap) | Y (style gap) | Y (style gap) |
| 99 CREATECOLORSPACE | slot | — | — | slot |
| 108 SMALLTEXTOUT | — | — | Y | Y |
| 114 ALPHABLEND | Y | Y | Y | Y |
| 122 CREATECOLORSPACEW | slot | — | — | slot |

## WMF records (hex IDs)

| Record | VectorCraft import | WordCraft | DeckCraft | Shared |
|---|---|---|---|---|
| 0x0000 EOF | Y | Y | Y | Y |
| 0x001E SAVEDC | Y | Y | Y | Y |
| 0x00F7 CREATEPALETTE | slot | slot | slot | slot |
| 0x0102 SETBKMODE | — | — | Y | Y |
| 0x0103 SETMAPMODE | — | Y | Y | Y |
| 0x0104 SETROP2 | — | Y | Y | Y |
| 0x0106 SETPOLYFILLMODE | Y | Y | Y | Y |
| 0x0127 RESTOREDC | Y | Y | Y | Y |
| 0x012D SELECTOBJECT | Y | Y | Y | Y |
| 0x012E SETTEXTALIGN | Y | — | Y | Y |
| 0x0142 DIBCREATEPATTERNBRUSH | slot | slot | Y | Y |
| 0x01F0 DELETEOBJECT | Y | Y | Y | Y |
| 0x01F9 CREATEPATTERNBRUSH | slot | slot | Y | Y |
| 0x0201 SETBKCOLOR | — | — | Y | Y |
| 0x0209 SETTEXTCOLOR | Y | — | Y | Y |
| 0x020B SETWINDOWORG | placeable only | Y | Y | Y |
| 0x020C SETWINDOWEXT | placeable only | Y | Y | Y |
| 0x020D SETVIEWPORTORG | — | Y | Y | Y |
| 0x020E SETVIEWPORTEXT | — | Y | Y | Y |
| 0x0213 LINETO | Y | Y | Y | Y |
| 0x0214 MOVETO | Y | Y | Y | Y |
| 0x02FA CREATEPENINDIRECT | Y | Y | Y | Y |
| 0x02FB CREATEFONTINDIRECT | Y | slot | Y | Y |
| 0x02FC CREATEBRUSHINDIRECT | solid/none | solid/none | Y | Y |
| 0x0324 POLYGON | Y | Y | Y | Y |
| 0x0325 POLYLINE | Y | Y | Y | Y |
| 0x0415 EXCLUDECLIPRECT | partial | — | Y | Y |
| 0x0416 INTERSECTCLIPRECT | Y | — | Y | Y |
| 0x0418 ELLIPSE | Y | Y | Y | Y |
| 0x041B RECTANGLE | Y | Y | Y | Y |
| 0x0521 TEXTOUT | Y | — | Y | Y |
| 0x0538 POLYPOLYGON | Y | Y | Y | Y |
| 0x061C ROUNDRECT | Y | Y | Y | Y |
| 0x061D PATBLT | Y | — | Y | Y |
| 0x06FF CREATEREGION | slot | slot | slot | slot |
| 0x0817 ARC | Y | Y | Y | Y |
| 0x081A PIE | Y | Y | Y | Y |
| 0x0830 CHORD | Y | Y | Y | Y |
| 0x0940 DIBBITBLT | Y | Y | Y | Y |
| 0x0A32 EXTTEXTOUT | Y | — | Y | Y |
| 0x0B41 DIBSTRETCHBLT | Y | Y | Y | Y |
| 0x0F43 STRETCHDIB | Y | Y | Y | Y |

## EMF+ carried by EMR_COMMENT

WordCraft has no EMF+ reader; VectorCraft detects the signature only. DeckCraft
and Shared play the same subset below. Dual files choose plain EMF playback to
avoid drawing twice. EMF+-only files play the plus records.

| Record(s) | VectorCraft import | WordCraft | DeckCraft | Shared |
|---|---|---|---|---|
| 0x4001 Header | detect container | — | Y | Y |
| 0x4008 Object (including continuations) | — | — | Y | Y, aggregate limits |
| 0x4009 Clear; 0x400A/B Fill/DrawRects | — | — | Y | Y |
| 0x400C/D FillPolygon/DrawLines | — | — | Y | Y |
| 0x400E/F Fill/DrawEllipse | — | — | Y | Y |
| 0x4010–12 FillPie/DrawPie/DrawArc | — | — | Y | Y |
| 0x4013 FillRegion; 0x4014/15 Fill/DrawPath | — | — | Y | Y |
| 0x4016–19 closed/open cardinal curves, Béziers | — | — | Y | Y |
| 0x401A/B DrawImage/DrawImagePoints | — | — | Y | Y, bounded decoding/nesting |
| 0x401C DrawString; 0x4036 DrawDriverString | — | — | partial | partial |
| 0x4025–29 save/restore/begin/end containers | — | — | Y | Y |
| 0x402A–30 world/page transforms | — | — | Y | Y |
| 0x4031–35 reset/set/offset clipping | — | — | partial | partial |
| 0x4002–7, 0x401D–24 hints/attributes; 0x4037–3A effects/TS | — | — | skipped | skipped |

## Consolidation decisions and gaps

- All plain Bézier variants already preserved cubics in DeckCraft and WordCraft.
  No curve flattening fix was needed there. Added VectorCraft-only 31/32 extent
  scaling, 57 direction (saved/restored), 68 abort-path, and 49/99/122 slots.
- WordCraft's changed implementations, not just record numbers, mattered:
  crop-first DIB decode and bottom-up crop placement, narrow crop windows,
  empty poly-group allocation, object-slot reuse, and `Text::layout` were folded
  in. DeckCraft's richer text transformations, pattern fills, regions and EMF+
  remain. No internal dependency required replacement.
- VectorCraft retains pen dashes/caps/joins/miter and arbitrary path clips.
  Shared's simple color/width stroke and rectangular GDI region model cannot
  retain those: 58 SETMITERLIMIT and 67 SELECTCLIPPATH remain explicit gaps.
  VectorCraft's exclusion branch only approximates exclusion; Shared retains
  DeckCraft's rectangle-region subtraction.
- EMF+ nonconvex clipping falls back to a bounding rectangle; intersecting
  crossing curves are flattened. Region XOR/exclude/complement combinations
  approximate the left area. Non-axis-aligned image placement is approximated.
  Driver glyph indices work only for supported Windows core fonts. Rendering
  hints, image attributes/effects, TS extensions and undocumented StrokeFillPath
  remain skipped. Missing glyph resolution belongs to the caller.
- Plain DIB RLE/JPEG/PNG compression is not supported (EMF+ compressed image
  objects are). Destination-dependent raster operations approximate white paper.
  Full GDI fidelity, app dependency migrations, VectorCraft's writer and real-world
  binary corpus acquisition are outside this library-only ticket.

The eight synthetic fixtures, seven live-recorded WordCraft path-bound baselines,
the explicit WordCraft EMF+ absence, both original test suites, and 16,000 seeded
mutations are described in tests/fixtures/README.md. This comparison is regression
coverage, not proof that every supported record renders identically across apps.
