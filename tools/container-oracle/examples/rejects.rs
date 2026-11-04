//! Lists corpus compound files that craft-cfb rejects, with what the other readers say.
use container_oracle::harness::discover;
use std::io::Cursor;
fn main() {
    let root = std::env::args().nth(1).expect("corpus root");
    let files = discover(std::path::Path::new(&root));
    for (name, path) in &files.cfb {
        let b = std::fs::read(path).unwrap();
        let new = craft_cfb::Cfb::open(&b).map(|_| ()).map_err(|e| e.0);
        let grid = container_oracle::cfbside::gridcraft::streams_old(&b).unwrap().map(|_| ());
        let guard = container_oracle::old_wordcraft_cfb_guard::check(&b);
        let refc = guard.clone().map_err(|e| format!("guard: {e}")).and_then(|_| cfb_ref::CompoundFile::open(Cursor::new(&b)).map(|_| ()).map_err(|e| e.to_string()));
        let film = container_oracle::cfbside::filmcraft::old(&b).map(|r| r.map(|_| ()));
        let fnew = container_oracle::cfbside::filmcraft::new(&b).map(|r| r.map(|_| ()));
        let accepted_old = grid.is_ok() || refc.is_ok() || matches!(film, Ok(Ok(_)));
        if new.is_err() && accepted_old || new.is_ok() && !(grid.is_ok() && refc.is_ok()) {
            println!("{name}\n   craft-cfb {new:?}\n   gridcraft-old {grid:?}\n   cfb-crate {refc:?}\n   filmcraft-old {film:?}");
        }
    }
}
