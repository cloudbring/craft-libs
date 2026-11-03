import CraftBlend

open CraftBlend

def modeName : Mode → String
  | .normal => "normal"
  | .multiply => "multiply"
  | .screen => "screen"
  | .overlay => "overlay"
  | .darken => "darken"
  | .lighten => "lighten"
  | .colorDodge => "colorDodge"
  | .colorBurn => "colorBurn"
  | .hardLight => "hardLight"
  | .softLight => "softLight"
  | .difference => "difference"
  | .exclusion => "exclusion"

def fraction (value : ℚ) : String := s!"{value.num}/{value.den}"

def emit (mode : Mode) (backdrop source backdropAlpha sourceAlpha : ℚ) : IO Unit := do
  match blendQ mode backdrop source with
  | none => pure ()
  | some mixed =>
    let outputAlpha := alpha backdropAlpha sourceAlpha
    let outputPremul := premul backdrop source backdropAlpha sourceAlpha mixed
    let outputColor := straight backdrop source backdropAlpha sourceAlpha mixed
    IO.println (String.intercalate "," ([modeName mode] ++
      [backdrop, source, backdropAlpha, sourceAlpha, mixed, outputPremul, outputColor, outputAlpha].map fraction))

def generate : IO Unit := do
  IO.println "mode,cb,cs,ab,as,blend,premul,color,alpha"
  let colors : List ℚ := [0, 1 / 16, 1 / 4, 1 / 2, 9 / 16, 3 / 4, 1]
  let alphas : List (ℚ × ℚ) := [(0, 0), (0, 1 / 2), (1 / 2, 0), (1 / 2, 1 / 2),
    (1, 1), (1 / 4, 3 / 4), (1 / 1000000, 1 / 1000000)]
  for mode in modes do
    for backdrop in colors do
      for source in colors do
        for (backdropAlpha, sourceAlpha) in alphas do
          emit mode backdrop source backdropAlpha sourceAlpha
  emit .multiply (1 / 2) (4 / 5) (1 / 2) (1 / 2)

#eval generate
