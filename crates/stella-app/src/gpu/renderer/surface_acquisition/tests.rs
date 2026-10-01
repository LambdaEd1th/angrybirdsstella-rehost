use super::*;
use std::collections::VecDeque;

struct Window {
    statuses: VecDeque<CurrentSurfaceTexture>,
    recoveries: Vec<SurfaceRecovery>,
    acquisitions: usize,
}

fn acquire(window: &mut Window) -> Result<CurrentSurfaceTexture> {
    window.acquisitions += 1;
    Ok(window
        .statuses
        .pop_front()
        .expect("unexpected extra acquire"))
}

fn recover(window: &mut Window, recovery: SurfaceRecovery) -> Result<()> {
    window.recoveries.push(recovery);
    Ok(())
}

#[test]
fn transient_acquisition_after_recovery_defers_instead_of_stopping_the_game() {
    for lost in [false, true] {
        for occluded in [false, true] {
            let initial = if lost {
                CurrentSurfaceTexture::Lost
            } else {
                CurrentSurfaceTexture::Outdated
            };
            let transient = if occluded {
                CurrentSurfaceTexture::Occluded
            } else {
                CurrentSurfaceTexture::Timeout
            };
            let mut window = Window {
                statuses: VecDeque::from([initial, transient]),
                recoveries: Vec::new(),
                acquisitions: 0,
            };
            let frame = acquire_window_frame(&mut window, None, acquire, recover).unwrap();
            assert!(frame.texture.is_none());
            assert_eq!(frame.pending_recovery, None);
            assert_eq!(window.acquisitions, 2);
            assert_eq!(
                window.recoveries,
                [if lost {
                    SurfaceRecovery::Recreate
                } else {
                    SurfaceRecovery::Reconfigure
                }]
            );
        }
    }
}

#[test]
fn repeated_surface_changes_are_bounded_and_recovered_on_the_next_redraw() {
    let mut window = Window {
        statuses: VecDeque::from([
            CurrentSurfaceTexture::Outdated,
            CurrentSurfaceTexture::Lost,
            CurrentSurfaceTexture::Outdated,
            CurrentSurfaceTexture::Occluded,
            CurrentSurfaceTexture::Timeout,
        ]),
        recoveries: Vec::new(),
        acquisitions: 0,
    };
    let first = acquire_window_frame(&mut window, None, acquire, recover).unwrap();
    assert!(first.texture.is_none());
    assert_eq!(first.pending_recovery, Some(SurfaceRecovery::Recreate));
    assert_eq!(window.acquisitions, 2);
    assert_eq!(window.recoveries, [SurfaceRecovery::Reconfigure]);
    let second =
        acquire_window_frame(&mut window, first.pending_recovery, acquire, recover).unwrap();
    assert!(second.texture.is_none());
    assert_eq!(second.pending_recovery, Some(SurfaceRecovery::Reconfigure));
    assert_eq!(window.acquisitions, 3);
    assert_eq!(
        window.recoveries,
        [SurfaceRecovery::Reconfigure, SurfaceRecovery::Recreate]
    );
    let third =
        acquire_window_frame(&mut window, second.pending_recovery, acquire, recover).unwrap();
    assert!(third.texture.is_none());
    assert_eq!(third.pending_recovery, None);
    assert_eq!(window.acquisitions, 4);
    let fourth = acquire_window_frame(&mut window, None, acquire, recover).unwrap();
    assert!(fourth.texture.is_none());
    assert_eq!(window.acquisitions, 5);
    assert_eq!(window.recoveries.len(), 3);
}

#[test]
fn validation_and_recovery_failures_remain_errors_without_an_extra_acquire() {
    for initial in [None, Some(CurrentSurfaceTexture::Lost)] {
        let mut statuses = VecDeque::new();
        statuses.extend(initial);
        statuses.push_back(CurrentSurfaceTexture::Validation);
        let mut window = Window {
            statuses,
            recoveries: Vec::new(),
            acquisitions: 0,
        };
        let result = acquire_window_frame(&mut window, None, acquire, recover);
        assert!(result.unwrap_err().to_string().contains("validation"));
        assert_eq!(window.acquisitions, window.recoveries.len() + 1);
    }
    let mut window = Window {
        statuses: VecDeque::from([CurrentSurfaceTexture::Lost]),
        recoveries: Vec::new(),
        acquisitions: 0,
    };
    let result = acquire_window_frame(&mut window, None, acquire, |_, _| {
        Err(anyhow!("create surface failed"))
    });
    assert_eq!(result.unwrap_err().to_string(), "create surface failed");
    assert_eq!(window.acquisitions, 1);
}

#[test]
fn zero_window_extent_preserves_pending_recovery_without_acquiring() {
    let mut renderer = crate::gpu::GpuRenderer::headless(crate::GameResolution {
        width: 4,
        height: 4,
    })
    .unwrap();
    renderer.surface_recovery = Some(SurfaceRecovery::Recreate);
    for (width, height) in [(0, 0), (0, 4), (4, 0)] {
        renderer
            .present_window_with_acquire_for_test(width, height, |_| {
                panic!("acquired a zero-size window")
            })
            .unwrap();
        assert_eq!(renderer.surface_recovery, Some(SurfaceRecovery::Recreate));
    }
}
