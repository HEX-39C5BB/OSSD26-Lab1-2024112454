use crate::{BlockPos, HORIZONTAL_NEIGHBORS};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireGeometry {
    /// Connected faces in the order of `HORIZONTAL_NEIGHBORS`.
    pub ports: u8,
    /// Connected faces with a vertical segment.
    pub ascending: u8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WireCell {
    Open,
    Solid,
    Dust,
}

/// Find a dust-to-dust connection across a one-block step. The caller checks
/// same-height ports first and supplies cells from its current world view.
pub fn stepped_wire(
    origin: BlockPos,
    direction: usize,
    mut cell: impl FnMut(BlockPos) -> Option<WireCell>,
) -> Option<(BlockPos, bool)> {
    let [dx, _, dz] = HORIZONTAL_NEIGHBORS[direction];
    let side = BlockPos::new(
        origin.x.checked_add(dx)?,
        origin.y,
        origin.z.checked_add(dz)?,
    );
    match cell(side)? {
        WireCell::Solid => {
            let y = origin.y.checked_add(1)?;
            let high = BlockPos::new(side.x, y, side.z);
            (cell(BlockPos::new(origin.x, y, origin.z))? != WireCell::Solid
                && cell(high)? == WireCell::Dust)
                .then_some((high, true))
        }
        WireCell::Open => {
            let y = origin.y.checked_sub(1)?;
            let low = BlockPos::new(side.x, y, side.z);
            (cell(BlockPos::new(origin.x, y, origin.z))? == WireCell::Solid
                && cell(low)? == WireCell::Dust)
                .then_some((low, false))
        }
        WireCell::Dust => None,
    }
}
