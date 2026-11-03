//! First-party provenance: DeckCraft `crates/metafile/src/lib.rs`, originally ported from WordCraft.
use crate::Text;
use std::sync::Arc;

/// A parsed metafile. Items use coordinates in `0..width` and `0..height`, y down.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    /// Size of the drawing in its own units: points for placeable WMF and EMF (from the frame), device
    /// or logical units otherwise.
    pub width: f32,
    pub height: f32,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A filled and/or stroked path, in picture coordinates.
    Path {
        segs: Vec<Seg>,
        fill: Option<[u8; 4]>,
        /// Colour and width in picture units.
        stroke: Option<([u8; 4], f32)>,
        even_odd: bool,
        /// Clip region in picture units, each rectangle `[x0, y0, x1, y1]`. None is the default clip.
        region: Option<Vec<[f32; 4]>>,
    },
    /// A bitmap stretched into `rect` = `[x, y, w, h]` (picture units), RGBA8 rows top-down.
    Bitmap {
        rect: [f32; 4],
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        /// Clip region in picture units. None is the default clip.
        region: Option<Vec<[f32; 4]>>,
    },
    /// A path filled with a repeating tile (pattern and hatch brushes). Items filled with the same
    /// brush share its tile.
    Pattern {
        segs: Vec<Seg>,
        even_odd: bool,
        tile: Arc<Tile>,
        /// Clip region in picture units. None is the default clip.
        region: Option<Vec<[f32; 4]>>,
    },
    /// A text string.
    Text(Box<Text>),
}

/// A brush tile: RGBA8 rows top-down (transparent where the brush leaves the background), each
/// pixel `cell` picture units, with a tile's top-left corner at `origin`.
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub cell: [f32; 2],
    pub origin: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}
