# office-render-oracle

LibreOffice headless render oracle for DOCX, XLSX and PPTX (artcraft-algorithms#194). Standalone
cargo workspace: it is not a member of the craft-libs workspace and is not built by its CI.

For each input it produces two PDFs and compares them:

1. Reference: `soffice --headless --convert-to pdf` with a dedicated
   `-env:UserInstallation=file://<work>/lo-profile` (your own LibreOffice profile is never touched),
   one soffice at a time, per-file timeout, process group killed afterwards.
2. Ours: `wordcraft-cli convert IN OUT.pdf` / `gridcraft-cli convert ...` / `deckcraft-cli convert ...`
   (docx / xlsx / pptx). Build the three CLIs yourself at the revision under test and pass the
   directory with `--bin-dir`.
3. `pdftoppm -r 72` rasters (first 10 pages) and `pdftotext -layout` / `-bbox-layout` text.
4. `craft-compare`: per-page SSIM, CIEDE2000 mean, p99 abs error (images, padded to a common size with
   white); word F1 and reading order (whole-document text); line matching and median |dy|
   (`layout`, first 10 pages); page count.

LibreOffice is a reference, not ground truth. Every number is agreement with it.

```text
cargo run --release -- sample --manifests ../../crates/corpus/manifests --corpus ~/ArtCraft/corpus --seed 194 --out sample.json
cargo run --release -- run    --sample sample.json --work /scratch/work --bin-dir /scratch/bin --out results.jsonl
cargo run --release -- one    some.docx --work /scratch/work --bin-dir /scratch/bin
cargo run --release -- report --results results.jsonl --baseline baseline.json > report.md
cargo run --release -- triage --results results.jsonl --work /scratch/work --out /scratch/triage --n 10
```

Needs `soffice`, `pdftoppm`, `pdftotext` on `PATH`. `run` resumes: ids already in `--out` are skipped.
`--timeout` (default 90 s) bounds each LibreOffice run; our export gets at most 60 s.

Sampling is deterministic (splitmix64 from `--seed`); see
`docs/verification/office-render-oracle.md` for the method and the committed baseline.
