import CraftBezier

open CraftBezier

/-- Exact `n/d` in lowest terms. -/
def fraction (value : ℚ) : String := s!"{value.num}/{value.den}"

abbrev Pt := ℚ × ℚ

def q (n d : Int) : ℚ := (n : ℚ) / (d : ℚ)

/-- Named cubic control polygons (exact rationals). Easing curves are CSS `cubic-bezier` presets. -/
def polygons : List (String × List Pt) :=
  [ ("css-ease", [(0, 0), (q 1 4, q 1 10), (q 1 4, 1), (1, 1)]),
    ("css-ease-in", [(0, 0), (q 21 50, 0), (1, 1), (1, 1)]),
    ("css-ease-out", [(0, 0), (0, 0), (q 29 50, 1), (1, 1)]),
    ("css-ease-in-out", [(0, 0), (q 21 50, 0), (q 29 50, 1), (1, 1)]),
    ("quarter-circle", [(1, 0), (1, q 5523 10000), (q 5523 10000, 1), (0, 1)]),
    ("loop", [(0, 0), (3, 3), (-2, 3), (1, 0)]),
    ("s-curve", [(0, 0), (2, 3), (-1, -3), (1, 0)]),
    ("collinear", [(0, 0), (1, 1), (3, 3), (4, 4)]),
    ("point", [(q 1 3, q (-2) 7), (q 1 3, q (-2) 7), (q 1 3, q (-2) 7), (q 1 3, q (-2) 7)]),
    ("mixed-signs", [(q (-3) 2, q 5 3), (q 7 4, q (-9) 5), (q (-5) 3, q (-1) 2), (q 11 5, q 13 7)]) ]

/-- Subdivision parameters. -/
def params : List ℚ := [0, 1 / 8, 1 / 4, 1 / 3, 1 / 2, 2 / 3, 3 / 4, 7 / 8, 1]

/-- Cross-check parameters for the interpreter-level re-verification of the proved theorems. -/
def checkU : List ℚ := [0, 1 / 5, 1 / 2, 3 / 4, 1]

def pts (l : List Pt) : ℕ → Pt := fun i => l.getD i (0, 0)

def ptStr (p : Pt) : String := fraction p.1 ++ "," ++ fraction p.2

def row (name : String) (l : List Pt) (s : ℚ) : IO Unit := do
  let P := pts l
  let b : Pt := lvl 3 s P 0
  let left : List Pt := (List.range 4).map (leftPt s P)
  let right : List Pt := (List.range 4).map (rightPt 3 s P)
  -- Interpreter cross-checks of the proved statements (a failure aborts generation).
  if b != bez 3 P s then throw (IO.userError s!"de Casteljau != Bernstein: {name} {fraction s}")
  if left.getD 3 (0, 0) != b || right.getD 0 (0, 0) != b then
    throw (IO.userError s!"subdivision point mismatch: {name} {fraction s}")
  for u in checkU do
    if bez 3 (pts left) u != bez 3 P (s * u) then
      throw (IO.userError s!"left reparametrisation fails: {name} {fraction s} {fraction u}")
    if bez 3 (pts right) u != bez 3 P (s + (1 - s) * u) then
      throw (IO.userError s!"right reparametrisation fails: {name} {fraction s} {fraction u}")
  IO.println (String.intercalate ","
    ([name, fraction s] ++ (l.map ptStr) ++ [ptStr b] ++ (left.map ptStr) ++ (right.map ptStr)))

def header : String :=
  "name,s," ++ String.intercalate ","
    ((["p0", "p1", "p2", "p3", "b", "l0", "l1", "l2", "l3", "r0", "r1", "r2", "r3"]).flatMap
      fun n => [n ++ "x", n ++ "y"])

def generate : IO Unit := do
  IO.println header
  for (name, l) in polygons do
    for s in params do
      row name l s

#eval generate
