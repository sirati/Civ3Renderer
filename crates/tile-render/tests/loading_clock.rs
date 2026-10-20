use tile_render_shared::{FrameParams, HOST_CLOCK, free_frame_of};

#[test]
fn viewer_clock_advances_only_opted_in_sprites() {
    for frame in 0..30 {
        let start = frame * 83;
        assert_eq!(free_frame_of(777, start, HOST_CLOCK | 83, 0, 30), frame);
        assert_eq!(free_frame_of(0, start + 82, HOST_CLOCK | 83, 0, 30), frame);
    }
    assert_eq!(free_frame_of(0, 2490, HOST_CLOCK | 83, 0, 30), 0);
    for host in [0, 82, 83, 2490, u32::MAX] {
        assert_eq!(free_frame_of(17, host, 3, 2, 7), (17 / 3 + 2) % 7);
        assert_eq!(free_frame_of(17, host, 0, 2, 7), (17 + 2) % 7);
    }
}

#[test]
fn host_clock_uses_scalar_uniform_padding_within_existing_control_stride() {
    assert_eq!(std::mem::size_of::<FrameParams>(), 192);
    assert_eq!(std::mem::offset_of!(FrameParams, host_tick_ms), 176);
    assert_eq!(std::mem::offset_of!(FrameParams, _host_reserved0), 180);
    assert_eq!(std::mem::offset_of!(FrameParams, _host_reserved1), 184);
    assert_eq!(std::mem::offset_of!(FrameParams, _host_reserved2), 188);
    assert!(std::mem::size_of::<FrameParams>() <= 256);
}
