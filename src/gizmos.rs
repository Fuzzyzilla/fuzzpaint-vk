//! # Gizmos
//!
//! Represents an interactive overlay atop the document editing workspace but behind the UI workspace. Useful for interactive
//! elements, mouse hit detection, rendering immediate previews, ect. without re-rendering the document.
//!
//! More complex gizmos are made up of simple composable elements.
//
// (Todo: Should crate::document_viewport_proxy be a kind of gizmo? the parallels are clear...)

pub mod renderer;
pub mod transform;
use transform::*;

pub enum GizmoMeshMode {
    Triangles,
    LineStrip,
}
pub enum RenderShape {
    Rectangle {
        position: ultraviolet::Vec2,
        size: ultraviolet::Vec2,
        rotation: f32,
    },
    Ellipse {
        origin: ultraviolet::Vec2,
        radii: ultraviolet::Vec2,
        rotation: f32,
    },
}

/// How is a gizmo displayed?
/// For efficiency in rendering, the options are intentionally limited.
/// For more complex visuals, combined several gizmos in a group.
pub enum GizmoVisual {
    Shape {
        shape: RenderShape,
        /// The descriptor of the texture. Should image should be immutable, as read usage
        /// lifetime is not currently bounded.
        ///
        /// Set binding 0 should be the combined image sampler, which will be rendered with
        /// standard alpha blending.
        texture: Option<std::sync::Arc<crate::vk::PersistentDescriptorSet>>,
        /// RGBA color to multiply the whole gizmo by, linear with straight blending.
        color: [u8; 4],
    },
    /*
    Mesh {
        /// Interpret mesh as TriangleList or as a wide LineStrip?
        /// Texturing is supported for lines.
        style: GizmoMeshMode,
        /// The descriptor of the texture. Should image should be immutable, as read usage
        /// lifetime is not currently bounded.
        ///
        /// Set binding 0 should be the combined image sampler, which will be rendered with
        /// standard alpha blending.
        texture: Option<crate::vk::PersistentDescriptorSet>,
        /// Color modulation of this gizmo. Can be changed at will, and will be respected by the renderer.
        color: [f32; 4],
        /// A mesh of [renderer::GizmoVertex], forming primitives determined by `mode`.
        ///
        /// Coordinates are in the gizmo's local coordinate system, as determined by GizmoTransformPinning.
        ///
        /// Color will be multiplied with the texture sampled at UV (or white if no texture), and
        /// further multiplied by `Mesh::color`.
        mesh: (),
        /// Whether the mesh can mutate from frame-to-frame.
        /// If true, it will be re-uploaded to the GPU every frame,
        /// otherwise it may be cached and changes may be missed
        mutable: bool,
    },*/
    None,
}

/// How can a gizmo be interacted with by the mouse?
pub enum GizmoInteraction {
    None,
    /// Can be dragged, and arbitrarily constrained.
    Move,
    /// Can be clicked to open
    Open,
    /// Both `Move`-able and `Open`-able.
    MoveOpen,
    /// Can be rotated around its origin by dragging, can be arbitrarily constrained.
    Rotate,
}

/// The shape of a gizmo's hit window.
/// In local coordinates, determined by GizmoTransformPinning
pub enum GizmoShape {
    /// Hollow ring - can be used for circles when inner=0
    Ring {
        inner: f32,
        outer: f32,
    },
    Rectangle {
        min: [f32; 2],
        max: [f32; 2],
    },
    None,
}
impl GizmoShape {
    pub fn hit(&self, local: [f32; 2]) -> bool {
        match self {
            Self::None => false,
            Self::Rectangle {
                min: [x0, y0],
                max: [x1, y1],
            } => (local[0] > *x0 && local[0] < *x1) && (local[1] > *y0 && local[1] < *y1),
            Self::Ring { inner, outer } => {
                let dist_sq = local[0] * local[0] + local[1] * local[1];

                dist_sq > inner * inner && dist_sq < outer * outer
            }
        }
    }
}

/// A kind of inverse iterator, where the visitor will be passed down the whole
/// tree to visit every gizmo in order.
pub trait GizmoVisitor<T> {
    /// Visit a [Gizmo]. Return Some to short circuit, None to continue.
    fn visit_gizmo(&mut self, gizmo: &Gizmo) -> Option<T>;
    /// Visit a [Collection]. Return Some to short circuit, None to continue.
    fn visit_collection(&mut self, gizmo: &Collection) -> Option<T>;
    /// The most recent [Collection] has been fully visited. Return Some to short circuit, None to continue.
    fn end_collection(&mut self, gizmo: &Collection) -> Option<T>;
}

/// [GizmoVisitor] except it accesses the Gizmos as mutable references.
pub trait MutableGizmoVisitor<T> {
    /// Visit a [Gizmo]. Return Some to short circuit, None to continue.
    fn visit_gizmo_mut(&mut self, gizmo: &mut Gizmo) -> Option<T>;
    /// Visit a [Collection]. Return Some to short circuit, None to continue.
    fn visit_collection_mut(&mut self, gizmo: &mut Collection) -> Option<T>;
    /// The most recent [Collection] has been fully visited. Return Some to short circuit, None to continue.
    fn end_collection_mut(&mut self, gizmo: &mut Collection) -> Option<T>;
}

pub struct Gizmo {
    pub visual: GizmoVisual,

    pub interaction: GizmoInteraction,
    pub hit_shape: GizmoShape,

    pub hover_cursor: CursorOrInvisible,
    pub grab_cursor: CursorOrInvisible,

    pub transform: GizmoTransform,
}

/// A collection of many gizmos. It itself is a Gizmo,
/// meaning Collections-in-Collections is supported.
pub struct Collection {
    transform: GizmoTransform,
    /// Children of this gizmo, sorted top to bottom.
    children: Vec<AnyGizmo>,
}
impl Collection {
    pub fn new(transform: GizmoTransform) -> Self {
        Self {
            transform,
            children: Vec::new(),
        }
    }
    pub fn push_top(&mut self, other: impl Into<AnyGizmo>) {
        self.children.insert(0, other.into());
    }
    pub fn push_bottom(&mut self, other: impl Into<AnyGizmo>) {
        self.children.push(other.into());
    }
    fn evaluate_path_mut<'a>(&'a mut self, path: &'_ CollectionMeta) -> Option<&'a mut Gizmo> {
        let mut cur: Option<&'a mut [AnyGizmo]> = Some(&mut self.children);
        let mut found: Option<&'a mut Gizmo> = None;
        for idx in path.0.iter() {
            // Short circuit if we index too deep.
            let last_cur = cur?;
            // Short circuit if we index past-the end.
            let child = last_cur.get_mut(*idx)?;
            match child {
                // Index into this collection next
                AnyGizmo::Collection(c) => cur = Some(&mut c.children),
                // We found an endpoint -
                // will short circuit None next iteration if this wasn't the expected endpoint
                AnyGizmo::Gizmo(g) => {
                    cur = None;
                    found = Some(g);
                }
            }
        }
        found
    }
    fn evaluate_path<'a>(&'a self, path: &'_ CollectionMeta) -> Option<&'a Gizmo> {
        let mut cur: Option<&'a [AnyGizmo]> = Some(&self.children);
        let mut found: Option<&'a Gizmo> = None;
        for idx in path.0.iter() {
            // Short circuit if we index too deep.
            let last_cur = cur?;
            // Short circuit if we index past-the end.
            let child = last_cur.get(*idx)?;
            match child {
                // Index into this collection next
                AnyGizmo::Collection(c) => cur = Some(&c.children),
                // We found an endpoint -
                // will short circuit None next iteration if this wasn't the expected endpoint
                AnyGizmo::Gizmo(g) => {
                    cur = None;
                    found = Some(g);
                }
            }
        }
        found
    }
}

// mem inefficient, implementation detail uwu
enum AnyGizmo {
    Gizmo(Gizmo),
    Collection(Collection),
}
enum AnyMeta {
    Gizmo(GizmoMeta),
    Collection(CollectionMeta),
}
impl From<Gizmo> for AnyGizmo {
    fn from(value: Gizmo) -> Self {
        Self::Gizmo(value)
    }
}
impl From<Collection> for AnyGizmo {
    fn from(value: Collection) -> Self {
        Self::Collection(value)
    }
}
impl From<GizmoMeta> for AnyMeta {
    fn from(value: GizmoMeta) -> Self {
        Self::Gizmo(value)
    }
}
impl From<CollectionMeta> for AnyMeta {
    fn from(value: CollectionMeta) -> Self {
        Self::Collection(value)
    }
}

mod seal {
    pub trait _Sealed {}
    impl _Sealed for super::AnyGizmo {}
    impl _Sealed for super::Gizmo {}
    impl _Sealed for super::Collection {}
}

use winit::window::CursorIcon;
/// None to hide the cursor, or Some to choose a winit cursor.
#[derive(Copy, Clone)]
pub enum CursorOrInvisible {
    Icon(CursorIcon),
    Invisible,
}
// Idk what to name this lol
/// Sealed, because we assume Gizmo and GizmoCollection are the only two valid
/// gizmos. Just keeps logic clean, and that's the whole point of the composable style
/// of this API :3
pub trait Gizmooooo: seal::_Sealed {
    type Meta;
    /// Bounding box for hit checks, in the parent's coordinate space. Purely optimization, None is always valid.
    /// Gizmos may escape their bounding box visually, but inputs *may* be skipped.
    fn hit_bounding_box(&self) -> Option<()>;
    /// If hovering this local coordinate, what cursor do we show? Or None to pass-thru.
    fn cursor_at(&self, point: [f32; 2]) -> Option<CursorOrInvisible>;
    /// While grabbed with this path, what cursor do we show?
    fn grabbed_cursor(&self, path: &Self::Meta) -> CursorOrInvisible;
    /// A click was registered to this gizmo - return some metadata to allow future tracking,
    /// or None to pass-thru the click event.
    ///
    /// point is in the gizmo's local coordinates.
    fn click_at(&mut self, point: [f32; 2]) -> Option<Self::Meta>;
    /// The mouse dragged by delta viewport pixels after a click.
    ///
    /// May be smaller or larger than the physical distance travelled by the
    /// mouse, to allow things like holding ctrl to drag more precisely or shift to drag more coursely.
    fn dragged_delta(&mut self, path: &Self::Meta, delta: [f32; 2]);
    /// The mouse stopped dragging. Returns ownership of the Meta given when the
    /// mouse first clicked this gizmo.
    fn drag_release(&mut self, path: Self::Meta);
    /// The mouse clicked the gizmo. Drags may have been emitted, but it is retroactively treated
    /// as a click instead. This is detected for example if the cumulative drag delta is sufficiently small after releasing.
    fn click_release(&mut self, path: Self::Meta);
    /// Pass the visitor to self and all children!
    /// Should visit in painters order, back-to-front.
    /// Returns Some with the short circuit value, or None if never short circuited.
    fn visit_painter<T>(&self, visitor: &mut impl GizmoVisitor<T>) -> Option<T>;
    /// Pass the visitor to self and all children!
    /// Should visit in hit order, front-to-back.
    /// Returns Some with the short circuit value, or None if never short circuited.
    fn visit_hit<T>(&self, visitor: &mut impl GizmoVisitor<T>) -> Option<T>;
    /// Pass the visitor to self and all children!
    /// Should visit in painters order, back-to-front.
    /// Returns Some with the short circuit value, or None if never short circuited.
    fn visit_painter_mut<T>(&mut self, visitor: &mut impl MutableGizmoVisitor<T>) -> Option<T>;
    /// Pass the visitor to self and all children!
    /// Should visit in hit order, front-to-back.
    /// Returns Some with the short circuit value, or None if never short circuited.
    fn visit_hit_mut<T>(&mut self, visitor: &mut impl MutableGizmoVisitor<T>) -> Option<T>;
}

// Possible types of path emitted by a gizmo.
pub struct GizmoMeta {
    /// Offset of the mouse at the time of mouse down from this gizmo's origin,
    /// in units determined by GizmoTransformPinning
    offset: [f32; 2],
}
/// Some number of indicies to drill down into the nested structure,
/// followed by the terminating gizmo metadata.
pub struct CollectionMeta(Vec<usize>, GizmoMeta);

impl Gizmooooo for Gizmo {
    type Meta = GizmoMeta;

    fn hit_bounding_box(&self) -> Option<()> {
        None
    }

    fn cursor_at(&self, point: [f32; 2]) -> Option<CursorOrInvisible> {
        self.hit_shape.hit(point).then_some(self.hover_cursor)
    }

    fn grabbed_cursor(&self, _path: &Self::Meta) -> CursorOrInvisible {
        self.grab_cursor
    }

    fn click_at(&mut self, point: [f32; 2]) -> Option<Self::Meta> {
        let meta = GizmoMeta { offset: point };

        self.hit_shape.hit(point).then_some(meta)
    }

    fn dragged_delta(&mut self, path: &Self::Meta, delta: [f32; 2]) {
        match self.interaction {
            GizmoInteraction::Move | GizmoInteraction::MoveOpen => {
                // todo: transform delta to local delta coords.
                self.transform.position[0] += delta[0];
                self.transform.position[1] += delta[1];
            }
            GizmoInteraction::Rotate => {
                // no transform needed.

                // A bit of a compromised solution for now :V
                // dragging right or up rotates clockwise,
                // left or down anticlockwise,
                // instead of working off the absolute position of mouse vs. self.
                const RAD_PER_PIXEL: f32 = 0.01;
                self.transform.rotation -= (delta[0] - delta[1]) * RAD_PER_PIXEL;
            }
            _ => (),
        }
    }

    fn drag_release(&mut self, _path: Self::Meta) {
        // Hmm.. I don't believe there is any work to do here :V
    }

    fn click_release(&mut self, _path: Self::Meta) {
        // That's a funny syntax :3
        if let GizmoInteraction::Open | GizmoInteraction::MoveOpen = self.interaction {
            // todo: Open self
        }
    }

    fn visit_painter<T>(&self, visitor: &mut impl GizmoVisitor<T>) -> Option<T> {
        visitor.visit_gizmo(self)
    }

    fn visit_hit<T>(&self, visitor: &mut impl GizmoVisitor<T>) -> Option<T> {
        visitor.visit_gizmo(self)
    }

    fn visit_painter_mut<T>(&mut self, visitor: &mut impl MutableGizmoVisitor<T>) -> Option<T> {
        visitor.visit_gizmo_mut(self)
    }

    fn visit_hit_mut<T>(&mut self, visitor: &mut impl MutableGizmoVisitor<T>) -> Option<T> {
        visitor.visit_gizmo_mut(self)
    }
}

impl Gizmooooo for Collection {
    type Meta = CollectionMeta;

    fn hit_bounding_box(&self) -> Option<()> {
        None
    }

    fn cursor_at(&self, point: [f32; 2]) -> Option<CursorOrInvisible> {
        struct CursorFindVisitor {
            point_stack: Vec<[f32; 2]>,
        }
        impl GizmoVisitor<CursorOrInvisible> for CursorFindVisitor {
            fn visit_collection(&mut self, gizmo: &Collection) -> Option<CursorOrInvisible> {
                // todo: transform point.
                let xformed = *self.point_stack.last().unwrap();
                self.point_stack.push(xformed);
                None
            }
            fn end_collection(&mut self, _: &Collection) -> Option<CursorOrInvisible> {
                self.point_stack.pop();
                None
            }
            fn visit_gizmo(&mut self, gizmo: &Gizmo) -> Option<CursorOrInvisible> {
                // todo: transform point.
                let xformed = *self.point_stack.last().unwrap();
                // Short circuits the iteration if this returns Some
                gizmo.cursor_at(xformed)
            }
        }
        let mut visitor = CursorFindVisitor {
            point_stack: vec![point],
        };

        // Visitor will find the correct icon to use, or None if no gizmos asserted an icon.
        self.visit_hit(&mut visitor)
    }

    fn click_at(&mut self, point: [f32; 2]) -> Option<Self::Meta> {
        // Recursively search the collection structure, populating path and returning Some if
        // a gizmo is found that accepted the click.

        struct ClickVisitor {
            path: smallvec::SmallVec<[usize; 4]>,
            points_stack: Vec<[f32; 2]>,
        }
        impl MutableGizmoVisitor<CollectionMeta> for ClickVisitor {
            fn visit_collection_mut(&mut self, gizmo: &mut Collection) -> Option<CollectionMeta> {
                // Advance the last path idx
                *self.path.last_mut().unwrap() += 1;
                // Start a new nested path
                self.path.push(0);

                // todo: transform
                let xformed = *self.points_stack.last().unwrap();
                self.points_stack.push(xformed);
                None
            }
            fn visit_gizmo_mut(&mut self, gizmo: &mut Gizmo) -> Option<CollectionMeta> {
                // todo: transform
                let xformed = *self.points_stack.last().unwrap();
                match gizmo.click_at(xformed) {
                    Some(meta) => Some(CollectionMeta(
                        std::mem::take(&mut self.path).to_vec(),
                        meta,
                    )),
                    None => {
                        *self.path.last_mut().unwrap() += 1;
                        None
                    }
                }
            }
            fn end_collection_mut(&mut self, _: &mut Collection) -> Option<CollectionMeta> {
                // Clear last nested path
                self.path.pop();
                self.points_stack.pop();

                None
            }
        }

        let mut visitor = ClickVisitor {
            path: smallvec::smallvec![0],
            points_stack: vec![point],
        };

        self.visit_hit_mut(&mut visitor)
    }

    fn grabbed_cursor(&self, path: &Self::Meta) -> CursorOrInvisible {
        if let Some(gizmo) = self.evaluate_path(path) {
            gizmo.grabbed_cursor(&path.1)
        } else {
            CursorOrInvisible::Icon(CursorIcon::Help)
        }
    }

    fn dragged_delta(&mut self, path: &Self::Meta, delta: [f32; 2]) {
        if let Some(gizmo) = self.evaluate_path_mut(path) {
            gizmo.dragged_delta(&path.1, delta)
        }
    }

    fn drag_release(&mut self, path: Self::Meta) {
        if let Some(gizmo) = self.evaluate_path_mut(&path) {
            gizmo.drag_release(path.1)
        }
    }

    fn click_release(&mut self, path: Self::Meta) {
        if let Some(gizmo) = self.evaluate_path_mut(&path) {
            gizmo.click_release(path.1)
        }
    }

    fn visit_painter<T>(&self, visitor: &mut impl GizmoVisitor<T>) -> Option<T> {
        if let Some(t) = visitor.visit_collection(self) {
            return Some(t);
        };

        // In painters order- reverse the children
        for child in self.children.iter().rev() {
            if let Some(t) = child.visit_painter(visitor) {
                return Some(t);
            }
        }

        visitor.end_collection(self)
    }

    fn visit_hit<T>(&self, visitor: &mut impl GizmoVisitor<T>) -> Option<T> {
        if let Some(t) = visitor.visit_collection(self) {
            return Some(t);
        };

        // In hit order- don't reverse the children
        for child in self.children.iter() {
            if let Some(t) = child.visit_hit(visitor) {
                return Some(t);
            }
        }

        visitor.end_collection(self)
    }

    fn visit_painter_mut<T>(&mut self, visitor: &mut impl MutableGizmoVisitor<T>) -> Option<T> {
        if let Some(t) = visitor.visit_collection_mut(self) {
            return Some(t);
        };

        // In painters order- reverse the children
        for child in self.children.iter_mut().rev() {
            if let Some(t) = child.visit_painter_mut(visitor) {
                return Some(t);
            }
        }

        visitor.end_collection_mut(self)
    }

    fn visit_hit_mut<T>(&mut self, visitor: &mut impl MutableGizmoVisitor<T>) -> Option<T> {
        if let Some(t) = visitor.visit_collection_mut(self) {
            return Some(t);
        };

        // In hit order- don't reverse the children
        for child in self.children.iter_mut() {
            if let Some(t) = child.visit_hit_mut(visitor) {
                return Some(t);
            }
        }

        visitor.end_collection_mut(self)
    }
}

impl Gizmooooo for AnyGizmo {
    type Meta = AnyMeta;

    fn hit_bounding_box(&self) -> Option<()> {
        match self {
            AnyGizmo::Collection(g) => g.hit_bounding_box(),
            AnyGizmo::Gizmo(g) => g.hit_bounding_box(),
        }
    }

    fn cursor_at(&self, point: [f32; 2]) -> Option<CursorOrInvisible> {
        match self {
            AnyGizmo::Collection(g) => g.cursor_at(point),
            AnyGizmo::Gizmo(g) => g.cursor_at(point),
        }
    }

    fn grabbed_cursor(&self, path: &Self::Meta) -> CursorOrInvisible {
        match (self, path) {
            (AnyGizmo::Collection(g), AnyMeta::Collection(m)) => g.grabbed_cursor(m),
            (AnyGizmo::Gizmo(g), AnyMeta::Gizmo(m)) => g.grabbed_cursor(m),
            _ => {
                log::warn!("Mismatched meta type in AnyGizmo::grabbed_cursor");
                CursorOrInvisible::Icon(CursorIcon::Help)
            }
        }
    }

    fn click_at(&mut self, point: [f32; 2]) -> Option<Self::Meta> {
        match self {
            AnyGizmo::Collection(g) => g.click_at(point).map(Into::into),
            AnyGizmo::Gizmo(g) => g.click_at(point).map(Into::into),
        }
    }

    fn dragged_delta(&mut self, path: &Self::Meta, delta: [f32; 2]) {
        match (self, path) {
            (AnyGizmo::Collection(g), AnyMeta::Collection(m)) => g.dragged_delta(m, delta),
            (AnyGizmo::Gizmo(g), AnyMeta::Gizmo(m)) => g.dragged_delta(m, delta),
            _ => {
                log::warn!("Mismatched meta type in AnyGizmo::dragged_delta");
            }
        }
    }

    fn drag_release(&mut self, path: Self::Meta) {
        match (self, path) {
            (AnyGizmo::Collection(g), AnyMeta::Collection(m)) => g.drag_release(m),
            (AnyGizmo::Gizmo(g), AnyMeta::Gizmo(m)) => g.drag_release(m),
            _ => {
                log::warn!("Mismatched meta type in AnyGizmo::drag_release");
            }
        }
    }

    fn click_release(&mut self, path: Self::Meta) {
        match (self, path) {
            (AnyGizmo::Collection(g), AnyMeta::Collection(m)) => g.click_release(m),
            (AnyGizmo::Gizmo(g), AnyMeta::Gizmo(m)) => g.click_release(m),
            _ => {
                log::warn!("Mismatched meta type in AnyGizmo::click_release");
            }
        }
    }

    fn visit_painter<T>(&self, visitor: &mut impl GizmoVisitor<T>) -> Option<T> {
        match self {
            AnyGizmo::Collection(g) => g.visit_painter(visitor),
            AnyGizmo::Gizmo(g) => g.visit_painter(visitor),
        }
    }

    fn visit_hit<T>(&self, visitor: &mut impl GizmoVisitor<T>) -> Option<T> {
        match self {
            AnyGizmo::Collection(g) => g.visit_hit(visitor),
            AnyGizmo::Gizmo(g) => g.visit_hit(visitor),
        }
    }

    fn visit_painter_mut<T>(&mut self, visitor: &mut impl MutableGizmoVisitor<T>) -> Option<T> {
        match self {
            AnyGizmo::Collection(g) => g.visit_painter_mut(visitor),
            AnyGizmo::Gizmo(g) => g.visit_painter_mut(visitor),
        }
    }

    fn visit_hit_mut<T>(&mut self, visitor: &mut impl MutableGizmoVisitor<T>) -> Option<T> {
        match self {
            AnyGizmo::Collection(g) => g.visit_hit_mut(visitor),
            AnyGizmo::Gizmo(g) => g.visit_hit_mut(visitor),
        }
    }
}