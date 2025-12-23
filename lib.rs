#![allow(clippy::missing_safety_doc)]
use core::ffi::c_char;

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct BridgeDlInstance {
    pub depth: u16,
    pub character_id: u16,
    pub x_px: i32,
    pub y_px: i32,
}

pub struct BridgePlayer {
    stage_w: i32,
    stage_h: i32,
    frames: u16,
    counts: Vec<u16>,
    offsets: Vec<u32>,
    instances: Vec<BridgeDlInstance>,
}

#[no_mangle]
pub extern "C" fn bridge_add(a: i64, b: i64) -> i64 { a + b }

#[no_mangle]
pub extern "C" fn bridge_version() -> *const c_char {
    b"bridge-director-0.1\0".as_ptr() as *const c_char
}

fn build_player_from_swf(swf_bytes: &[u8]) -> Result<BridgePlayer, i32> {
    use std::collections::BTreeMap;
    use std::io::Cursor;

    let swf_buf = swf::decompress_swf(Cursor::new(swf_bytes)).map_err(|_| -2)?;
    let movie = swf::parse_swf(&swf_buf).map_err(|_| -3)?;

    let stage = movie.header.stage_size();
    let stage_w = stage.width().to_pixels().round() as i32;
    let stage_h = stage.height().to_pixels().round() as i32;

    #[derive(Copy, Clone)]
    struct Node { id: u16, m: swf::Matrix }

    let mut dl: BTreeMap<u16, Node> = BTreeMap::new();

    const MAX_FRAMES: usize = 16384;
    const MAX_INST_TOTAL: usize = 1_000_000;

    let mut counts: Vec<u16> = Vec::new();
    let mut offsets: Vec<u32> = vec![0];
    let mut instances: Vec<BridgeDlInstance> = Vec::new();

    fn apply_place<'a>(po: &swf::PlaceObject<'a>, dl: &mut BTreeMap<u16, Node>) {
        let depth = po.depth;
        let new_m = po.matrix.unwrap_or(swf::Matrix::IDENTITY);

        match po.action {
            swf::PlaceObjectAction::Place(id) | swf::PlaceObjectAction::Replace(id) => {
                dl.insert(depth, Node { id, m: new_m });
            }
            swf::PlaceObjectAction::Modify => {
                if let Some(inst) = dl.get_mut(&depth) {
                    if po.matrix.is_some() {
                        inst.m = new_m;
                    }
                }
            }
        }
    }

    for tag in movie.tags.iter() {
        match tag {
            swf::Tag::PlaceObject(po) => apply_place(po, &mut dl),
            swf::Tag::RemoveObject(ro) => { dl.remove(&ro.depth); }

            swf::Tag::ShowFrame => {
                if counts.len() >= MAX_FRAMES { break; }
                let mut written: u16 = 0;

                for (depth, node) in dl.iter() {
                    if instances.len() >= MAX_INST_TOTAL { break; }
                    let tx = node.m.tx.to_pixels().round() as i32;
                    let ty = node.m.ty.to_pixels().round() as i32;

                    instances.push(BridgeDlInstance {
                        depth: *depth,
                        character_id: node.id,
                        x_px: tx,
                        y_px: ty,
                    });
                    written = written.wrapping_add(1);
                }

                counts.push(written);
                let next_off = offsets.last().copied().unwrap_or(0) + written as u32;
                offsets.push(next_off);
            }
            _ => {}
        }
    }

    let frames = counts.len().min(u16::MAX as usize) as u16;

    Ok(BridgePlayer { stage_w, stage_h, frames, counts, offsets, instances })
}

#[no_mangle]
pub extern "C" fn bridge_player_create(
    swf_ptr: *const u8,
    swf_len: usize,
    out_stage_w: *mut i32,
    out_stage_h: *mut i32,
    out_total_frames: *mut u16,
    out_total_instances: *mut u32,
) -> *mut BridgePlayer {
    if swf_ptr.is_null() || swf_len == 0 { return core::ptr::null_mut(); }
    let swf_bytes = unsafe { core::slice::from_raw_parts(swf_ptr, swf_len) };

    let player = match build_player_from_swf(swf_bytes) {
        Ok(p) => p,
        Err(_) => return core::ptr::null_mut(),
    };

    unsafe {
        if !out_stage_w.is_null() { *out_stage_w = player.stage_w; }
        if !out_stage_h.is_null() { *out_stage_h = player.stage_h; }
        if !out_total_frames.is_null() { *out_total_frames = player.frames; }
        if !out_total_instances.is_null() {
            *out_total_instances = player.instances.len().min(u32::MAX as usize) as u32;
        }
    }

    Box::into_raw(Box::new(player))
}

#[no_mangle]
pub extern "C" fn bridge_player_destroy(p: *mut BridgePlayer) {
    if p.is_null() { return; }
    unsafe { drop(Box::from_raw(p)); }
}

#[no_mangle]
pub extern "C" fn bridge_player_get_frame_instances(
    p: *mut BridgePlayer,
    frame_index: i32,
    out_instances: *mut BridgeDlInstance,
    out_instances_cap: usize,
    out_count: *mut u32,
) -> i32 {
    if p.is_null() || out_instances.is_null() || out_instances_cap == 0 { return -1; }
    let player = unsafe { &*p };
    if player.frames == 0 { return -2; }

    let mut fi = frame_index;
    if fi < 0 { fi = 0; }
    if fi >= player.frames as i32 { fi = player.frames as i32 - 1; }
    let fiu = fi as usize;

    let cnt = player.counts[fiu] as usize;
    let off = player.offsets[fiu] as usize;

    let copy_n = cnt.min(out_instances_cap);
    let src = &player.instances[off..off + copy_n];

    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), out_instances, copy_n);
        if !out_count.is_null() { *out_count = copy_n as u32; }
    }
    copy_n as i32
}