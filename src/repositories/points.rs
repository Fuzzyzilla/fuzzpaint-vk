//! # Points
//!
//! Points have the largest size footprint of all resources, due to how numerous they are.
//! Thus, it makes sense that their repository implementation should recieve the most care.
//! For now, the collection just grows unboundedly and no eviction is done -
//! however, the API is constructed to allow for smart in-memory compression or dumping old
//! data to disk in the future.

/// Get the shared global instance of the point repository.
pub fn global() -> &'static PointRepository {
    static REPO: std::sync::OnceLock<PointRepository> = std::sync::OnceLock::new();
    REPO.get_or_init(PointRepository::new)
}

bitflags::bitflags! {
    #[derive(Copy, Clone, Eq, PartialEq, Hash)]
    /// Description of a point's data fields. Organized such that devices that have later flags are
    /// also likely to have prior flags.
    ///
    /// Note that position nor arc_length are required fields. Arc_length is derived data,
    /// and position may be ignored for strokes which trace a predefined path.
    ///
    /// Selected (loosely) from 2D-drawing-relavent packets defined in the windows ink API:
    /// https://learn.microsoft.com/en-us/windows/win32/tablet/packetpropertyguids-constants
    #[rustfmt::skip]
    pub struct PointArchetype : u8 {
        /// The point stream reports an (X: f32, Y: f32) position.
        const POSITION =   0b0000_0001;
        /// The point stream reports an f32, representing the cumulative length of the path from the start.
        const ARC_LENGTH = 0b0000_0010;
        /// The point stream reports a normalized, non-saturated pressure value.
        const PRESSURE =   0b0000_0100;
        /// The point stream reports a signed noramlized (X: f32, Y: f32) tilt, where positive X is to the right,
        /// positive Y is towards the user.
        const TILT =       0b0000_1000;
        /// Alias for bits that contain two fields
        const TWO_FIELDS = 0b0000_1001;
        /// The point stream reports a normalized f32 distance, in arbitrary units.
        const DISTANCE =   0b0001_0000;
        /// The point stream reports stylus roll (rotation along it's axis). Units and sign unknown!
        ///
        /// FIXME: Someone with such hardware, please let me know what it's units are :3
        const ROLL =       0b0010_0000;
        /// The point stream reports wheel values in signed, unnormalized, non-wrapping degrees, f32.
        ///
        /// Wheels are a general-purpose value which the user can use in their brushes. It may
        /// correspond to a physical wheel on the pen or pad, a touch slider, ect. which may be interacted
        /// with during the stroke for expressive effects.
        const WHEEL =      0b0100_0000;
        /// f32, meaningless. Not associated with any axis, but available for one should a device expose
        /// another possible expressive axis that cannot already be assigned to any other field. As such, it could be
        /// used in the future!
        const UNASSIGNED = 0b1000_0000;
    }
}
impl PointArchetype {
    /// How many elements (f32) does a point of this archetype occupy?
    pub fn len(self) -> usize {
        // Formerly Self::iter based but the codegen was un-scrumptious

        // Every field specifies one element, count them all
        self.bits().count_ones() as usize
        // These fields specify two elements, count them again
            + (self & Self::TWO_FIELDS).bits().count_ones() as usize
    }
    pub fn len_bytes(self) -> usize {
        self.len() * std::mem::size_of::<f32>()
    }
}

#[derive(Copy, Clone)]
pub struct CollectionSummary {
    /// The archetype of the points of the collection.
    pub archetype: PointArchetype,
    /// Count of points within the collection
    pub len: usize,
    /// final arc length of the collected points, available if the archetype includes PointArchetype::ARC_LENGTH bit.
    pub arc_length: Option<f32>,
}

impl From<&[crate::StrokePoint]> for CollectionSummary {
    fn from(value: &[crate::StrokePoint]) -> Self {
        CollectionSummary {
            archetype: PointArchetype::POSITION
                | PointArchetype::ARC_LENGTH
                | PointArchetype::PRESSURE,
            len: value.len(),
            arc_length: Some(value.last().map(|point| point.dist).unwrap_or(0.0)),
        }
    }
}

pub struct PointCollectionIDMarker;
pub type PointCollectionID = crate::FuzzID<PointCollectionIDMarker>;

/// A handle for reading a collection of points. Can be cloned and shared between threads,
/// however take care not to allow it to become leaked - it will not allow the resources
/// to be reclaimed by the repository for the duration of the lock's lifetime.
#[derive(Clone)]
pub struct PointCollectionReadLock {
    points: &'static [crate::StrokePoint],
}
impl AsRef<[crate::StrokePoint]> for PointCollectionReadLock {
    // seal the detail that this is secretly 'static (shhhh...)
    fn as_ref<'a>(&'a self) -> &'a [crate::StrokePoint] {
        self.points
    }
}
impl std::ops::Deref for PointCollectionReadLock {
    type Target = [crate::StrokePoint];
    // seal the detail that this is secretly 'static (shhhh...)
    fn deref<'a>(&'a self) -> &'a Self::Target {
        self.points
    }
}

#[derive(thiserror::Error, Debug)]
pub enum WriteError {
    #[error("point collection {} is unknown", .0)]
    UnknownID(PointCollectionID),
    #[error("IO error {}", .0)]
    IOError(std::io::Error),
}
#[derive(Copy, Clone)]
struct PointCollectionAllocInfo {
    /// Which PointSlab is it in?
    /// (currently an index)
    slab_id: usize,
    /// What point index into that slab does it start?
    start: usize,
    /// A summary of the data within, that can be queried even if the bulk
    /// data is non-resident.
    summary: CollectionSummary,
}
pub struct PointRepository {
    slabs: parking_lot::RwLock<Vec<PointSlab>>,
    allocs: parking_lot::RwLock<hashbrown::HashMap<PointCollectionID, PointCollectionAllocInfo>>,
}
impl PointRepository {
    fn new() -> Self {
        // Self doesn't impl Default as we don't want any ctors to be public.
        Self {
            slabs: Default::default(),
            allocs: Default::default(),
        }
    }
    /// Get the memory usage of resident data (uncompressed in RAM), in bytes, and the capacity.
    pub fn resident_usage(&self) -> (usize, usize) {
        let read = self.slabs.read();
        let num_slabs = read.len();
        let capacity = num_slabs
            .saturating_mul(SLAB_SIZE)
            .saturating_mul(std::mem::size_of::<crate::StrokePoint>());
        let usage = read
            .iter()
            .map(|slab| slab.usage())
            .fold(0, usize::saturating_add)
            .saturating_mul(std::mem::size_of::<crate::StrokePoint>());
        (usage, capacity)
    }
    /// Insert the collection into the repository, yielding a unique ID.
    /// Fails if the length of the collection is > 0x10_00_00
    pub fn insert(&self, collection: &[crate::StrokePoint]) -> Option<PointCollectionID> {
        if collection.len() <= SLAB_SIZE {
            // Find a slab where `try_bump` succeeds.
            let slab_reads = self.slabs.upgradable_read();
            if let Some((slab_id, start)) = slab_reads
                .iter()
                .enumerate()
                .find_map(|(idx, slab)| Some((idx, slab.try_bump_write(collection)?)))
            {
                // We don't need this lock anymore!
                drop(slab_reads);

                // populate info
                let info = PointCollectionAllocInfo {
                    summary: collection.into(),
                    slab_id,
                    start,
                };
                // generate a new id and write metadata
                let id = PointCollectionID::default();
                self.allocs.write().insert(id, info);
                Some(id)
            } else {
                // No slabs were found with space to bump. Make a new one
                let new_slab = PointSlab::new();
                // Unwrap is infallible - we checked the size requirement, so there's certainly room!
                let start = new_slab.try_bump_write(collection).unwrap();
                // put the slab into self, getting it's index
                let slab_id = {
                    let mut write = parking_lot::RwLockUpgradableReadGuard::upgrade(slab_reads);
                    write.push(new_slab);
                    write.len() - 1
                };
                // populate info
                let info = PointCollectionAllocInfo {
                    summary: collection.into(),
                    slab_id,
                    start,
                };
                // generate a new id and write metadata
                let id = PointCollectionID::default();
                self.allocs.write().insert(id, info);
                Some(id)
            }
        } else {
            None
        }
    }
    /// Given an iterator of collection IDs, encodes them directly (in order) into the given Write stream, potentially skipping
    /// a round-trip decode-encode for non-resident data.
    pub fn write_into(
        &self,
        ids: impl Iterator<Item = PointCollectionID>,
        write: impl std::io::Write,
    ) -> Result<(), WriteError> {
        todo!()
    }
    /// Get a [CollectionSummary] for the given collection, reporting certain key aspects of a stroke without
    /// it needing to be loaded into resident memory. None if the ID is not known
    /// to this repository.
    pub fn summary_of(&self, id: PointCollectionID) -> Option<CollectionSummary> {
        self.allocs.read().get(&id).map(|info| info.summary)
    }
    pub fn try_get(
        &self,
        id: PointCollectionID,
    ) -> Result<PointCollectionReadLock, super::TryRepositoryError> {
        let alloc = self
            .allocs
            .read()
            .get(&id)
            .ok_or(super::TryRepositoryError::NotFound)?
            .clone();
        let slabs_read = self.slabs.read();
        let Some(slab) = slabs_read.get(alloc.slab_id) else {
            // Implementation bug!
            log::debug!("{id} allocation found, but slab doesn't exist!");
            return Err(super::TryRepositoryError::NotFound);
        };
        // Check the alloc range is reasonable
        debug_assert!(alloc
            .start
            .checked_add(alloc.summary.len)
            .is_some_and(|last| last <= SLAB_SIZE));

        let Some(slice) = slab.try_read(alloc.start, alloc.summary.len) else {
            // Implementation bug!
            log::debug!("{id} allocation found, but out of bounds within it's slab!");
            return Err(super::TryRepositoryError::NotFound);
        };
        Ok(PointCollectionReadLock { points: slice })
    }
}
// A large collection of continguous points on the heap
struct PointSlab {
    /// a non-null pointer to array of slab_SIZE points.
    points: *mut crate::StrokePoint,
    /// Current past-the-end index for the allocator.
    /// Indices before this are considered immutable, after are considered mutable.
    bump_free_idx: parking_lot::Mutex<usize>,
}
const SLAB_SIZE: usize = 1024 * 1024;
impl PointSlab {
    /// Try to allocate and write a contiguous section of points, returning the start idx of where it was written.
    /// If not enough space, the `self` is left unchanged and None is returned.
    fn try_bump_write(&self, data: &[crate::StrokePoint]) -> Option<usize> {
        if data.len() <= SLAB_SIZE {
            let mut free_idx = self.bump_free_idx.lock();
            let old_idx = *free_idx;
            let new_idx = old_idx.checked_add(data.len())?;
            if new_idx > SLAB_SIZE {
                None
            } else {
                // Safety - No shared mutable or immutable access can occur here,
                // since we own the mutex. Todo: could cause much pointless waiting for before the idx!
                let slice: &'static mut [crate::StrokePoint] =
                    unsafe { std::slice::from_raw_parts_mut(self.points.add(old_idx), data.len()) };
                slice
                    .iter_mut()
                    .zip(data.iter())
                    .for_each(|(into, from)| *into = *from);
                *free_idx = new_idx;
                Some(old_idx)
            }
        } else {
            None
        }
    }
    /// Try to read some continuous section of strokes. returns None if the region is outside the span
    /// of the currently allocated memory.
    ///
    /// Performs no check that the given start and length correspond to a single allocation!
    fn try_read(&self, start: usize, len: usize) -> Option<&'static [crate::StrokePoint]> {
        // Check if this whole region is within the allocated, read-only section.
        if start
            .checked_add(len)
            .is_some_and(|past_end| past_end <= *self.bump_free_idx.lock())
        {
            // Safety: no shared mutable access, as mutation never happens before the bump idx
            Some(unsafe { std::slice::from_raw_parts(self.points.add(start), len) })
        } else {
            None
        }
    }
    // Get the number of points currently in use.
    fn usage(&self) -> usize {
        *self.bump_free_idx.lock()
    }
    fn new() -> Self {
        let size = std::mem::size_of::<crate::StrokePoint>() * SLAB_SIZE;
        let align = std::mem::align_of::<crate::StrokePoint>();
        debug_assert!(size != 0);
        debug_assert!(align != 0 && align.is_power_of_two());

        // Safety: Size and align constraints ensured by debug asserts and unwraps.
        // (is there a better way to get a large zeroed heap array?)
        let points = unsafe {
            std::alloc::alloc_zeroed(std::alloc::Layout::from_size_align(size, align).unwrap())
                .cast::<crate::StrokePoint>()
        };
        assert!(!points.is_null());
        // We do not dealloc points at any point.
        // The slabs will be re-used for the lifetime of the program.
        Self {
            points,
            bump_free_idx: 0.into(),
        }
    }
}
// Safety - the pointer refers to heap mem, and can be transferred.
unsafe impl Send for PointSlab {}

// Safety - The mutex prevents similtaneous mutable and immutable access.
unsafe impl Sync for PointSlab {}
