//! Impl for Strokes and collections of strokes.

pub mod commands;
pub mod writer;

pub type StrokeCollectionID = crate::FuzzID<StrokeCollection>;
pub type ImmutableStrokeID = crate::FuzzID<ImmutableStroke>;

#[derive(Clone)]
pub struct ImmutableStroke {
    pub id: ImmutableStrokeID,
    pub brush: crate::state::StrokeBrushSettings,
    /// Points are managed and owned by the (point repository)[crate::repositories::points::PointRepository], not the stroke nor the queue.
    pub point_collection: crate::repositories::points::PointCollectionID,
}

#[derive(Clone)]
pub struct StrokeCollection {
    pub id: StrokeCollectionID,
    pub strokes: Vec<ImmutableStroke>,
    /// Flags to determine which strokes have are active/not "Undone"
    pub strokes_active: bitvec::vec::BitVec,
}
impl StrokeCollection {
    pub fn iter_active<'s>(&'s self) -> impl Iterator<Item = &'s ImmutableStroke> + 's {
        // Could also achieve with a zip. really depends on how dense we expect
        // deleted strokes to be, I should bench!
        self.strokes_active
            .iter_ones()
            // Short circuit iteration if we reach out-of-bounds (that'd be weird)
            .map_while(|index| self.strokes.get(index))
    }
    /// Insert a new stroke at the end, defaulting to active.
    fn push_back(&mut self, stroke: ImmutableStroke) {
        self.strokes.push(stroke);
        // Initially active.
        self.strokes_active.push(true);
    }
    // O(n).. I should do better :3
    // Can't binary search over IDs, as they're not technically
    // required to be ordered, in preparation for network shenanigans.
    /// Get a stroke by the given ID. Returns None if it is not found, or has been deleted.
    pub fn get(&self, id: ImmutableStrokeID) -> Option<&ImmutableStroke> {
        let (idx, stroke) = self
            .strokes
            .iter()
            .enumerate()
            .find(|(_, stroke)| stroke.id == id)?;

        // Return the stroke, if it's not deleted.
        self.strokes_active.get(idx)?.then_some(stroke)
    }
    /// Gets a mutable reference to a stroke, and it's activity status.
    fn get_mut<'s>(
        &'s mut self,
        id: ImmutableStrokeID,
    ) -> Option<(
        &mut ImmutableStroke,
        impl std::ops::DerefMut<Target = bool> + 's,
    )> {
        let (idx, stroke) = self
            .strokes
            .iter_mut()
            .enumerate()
            .find(|(_, stroke)| stroke.id == id)?;

        let active = self.strokes_active.get_mut(idx)?;

        Some((stroke, active))
    }
}
use crate::commands::{CommandConsumer, CommandError, DoUndo};
impl CommandConsumer<commands::StrokeCollectionCommand> for StrokeCollection {
    fn apply(
        &mut self,
        command: DoUndo<'_, commands::StrokeCollectionCommand>,
    ) -> Result<(), CommandError> {
        match command {
            DoUndo::Do(commands::StrokeCollectionCommand::Stroke(
                commands::StrokeCommand::Created {
                    target,
                    brush,
                    points,
                },
            )) => {
                const NEW_ACTIVE: bool = true;
                let (stroke, mut active) =
                    self.get_mut(*target).ok_or(CommandError::UnknownResource)?;

                // Was already set! Or, state doesn't match.
                if *active == NEW_ACTIVE
                    || stroke.point_collection != *points
                    || &stroke.brush != brush
                {
                    Err(CommandError::MismatchedState)
                } else {
                    *active = NEW_ACTIVE;
                    Ok(())
                }
            }
            DoUndo::Undo(commands::StrokeCollectionCommand::Stroke(
                commands::StrokeCommand::Created {
                    target,
                    brush,
                    points,
                },
            )) => {
                const NEW_ACTIVE: bool = false;
                let (stroke, mut active) =
                    self.get_mut(*target).ok_or(CommandError::UnknownResource)?;

                // Was already set! Or, state doesn't match.
                if *active == NEW_ACTIVE
                    || stroke.point_collection != *points
                    || &stroke.brush != brush
                {
                    Err(CommandError::MismatchedState)
                } else {
                    *active = NEW_ACTIVE;
                    Ok(())
                }
            }
        }
    }
}
