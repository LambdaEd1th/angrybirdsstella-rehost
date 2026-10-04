//! Compile the production browser retirement policy without desktop-only painters.
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[path = "../src/gpu/retirement.rs"]
mod retirement;
use retirement::retired_texture_names;

#[test]
fn last_input_frame_defers_retirement_after_a_release_packet_is_discarded() {
    let source = "<file-image-generation:1>fixture.png".to_owned();
    let binding = Arc::new(());
    let input_frame = binding.clone();
    let mut lifetimes = HashMap::from([(source.clone(), Arc::downgrade(&binding))]);
    let uploaded = HashSet::from([source.clone(), "<stella-white>".to_owned()]);
    let empty = HashSet::new();
    drop(binding);
    assert!(retired_texture_names(&empty, &uploaded, &lifetimes, &empty, &empty).is_empty());
    drop(input_frame);
    // The shared CPU collector already removed this generation and its Weak.
    // A discarded PreparedFrame must not lose the subsequent GL deletion.
    lifetimes.clear();
    assert_eq!(
        retired_texture_names(&empty, &uploaded, &lifetimes, &empty, &empty),
        HashSet::from([source])
    );
}

#[test]
fn recapture_keeps_the_current_image_until_its_owner_and_old_consumers_release() {
    let old = "<capture-generation:1:CAP>".to_owned();
    let current = "<capture-generation:2:CAP>".to_owned();
    let old_frame = Arc::new(());
    let binding = Arc::new(());
    let mut lifetimes = HashMap::from([
        (old.clone(), Arc::downgrade(&old_frame)),
        (current.clone(), Arc::downgrade(&binding)),
    ]);
    let mut uploaded = HashSet::from([old.clone(), current.clone()]);
    let empty = HashSet::new();
    let explicit = HashSet::from([old.clone()]);
    assert!(retired_texture_names(&explicit, &uploaded, &lifetimes, &empty, &empty).is_empty());
    drop(old_frame);
    assert_eq!(
        retired_texture_names(&explicit, &uploaded, &lifetimes, &empty, &empty),
        HashSet::from([old.clone()])
    );
    uploaded.remove(&old);
    lifetimes.remove(&old);
    drop(binding);
    assert_eq!(
        retired_texture_names(&empty, &uploaded, &lifetimes, &empty, &empty),
        HashSet::from([current])
    );
}

#[test]
fn current_stream_inputs_and_reproduced_capture_outputs_survive_retirement() {
    let input = "<file-image-generation:3>fixture.png".to_owned();
    let output = "<capture-generation:4:CAP>".to_owned();
    let unused = "<capture-generation:2:CAP>".to_owned();
    let uploaded = HashSet::from([input.clone(), output.clone(), unused.clone()]);
    let required = HashSet::from([input.clone()]);
    let produced = HashSet::from([output.clone()]);
    let lifetimes = HashMap::new();
    let empty = HashSet::new();
    assert_eq!(
        retired_texture_names(&uploaded, &uploaded, &lifetimes, &required, &produced),
        HashSet::from([unused])
    );
    assert_eq!(
        retired_texture_names(&empty, &uploaded, &lifetimes, &empty, &empty),
        uploaded
    );
}
