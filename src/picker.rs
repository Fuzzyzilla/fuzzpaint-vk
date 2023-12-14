//! Pickers allow the user to query single points at a time. Some ideas include selecting the top most stroke,
//! top layer, pick a color or brush from existing strokes, etc. Or just regular image pixel color picking!

pub trait Picker {
    /// What datatype does this picker yield when sampled?
    type Value;
    /// Pick at the given coordinate in the viewport. Returns None if the coordinate is outside
    /// the range of pickable values. The constructor of this type must then accept a transformation
    /// matrix to convert this coordiate to whatever internal space for sampling.
    fn pick(&self, viewport_coordinate: ultraviolet::Vec2) -> Option<Self::Value>;
}
