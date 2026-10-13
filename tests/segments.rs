use whistle_desktop::{audio::SAMPLE_RATE, segment::Segmenter};

#[test]
fn arbitrary_packets_preserve_every_sample_without_duplication() {
    let input: Vec<f32> = (0..SAMPLE_RATE * 71 + 117)
        .map(|i| {
            if i % 16000 < 1000 {
                0.0
            } else {
                (i as f32 * 0.017).sin() * 0.3
            }
        })
        .collect();
    for live in [false, true] {
        let mut segmenter = Segmenter::new(live);
        let mut output = Vec::new();
        let mut lengths = Vec::new();
        let mut emit = |pcm: &[f32]| {
            output.extend_from_slice(pcm);
            lengths.push(pcm.len());
            Ok(())
        };
        for packet in input.chunks(719) {
            segmenter.push(packet, &mut emit).unwrap();
        }
        segmenter.finish(&mut emit).unwrap();
        assert_eq!(output, input);
        assert!(lengths.len() > 5);
        assert!(
            lengths
                .iter()
                .all(|n| *n <= SAMPLE_RATE * if live { 6 } else { 12 })
        );
    }
}

#[test]
fn live_phrases_commit_after_half_a_second_of_silence() {
    let mut segmenter = Segmenter::new(true);
    segmenter
        .push(&vec![0.2; SAMPLE_RATE], &mut |_| {
            panic!("committed before a pause")
        })
        .unwrap();
    segmenter
        .push(&vec![0.0; SAMPLE_RATE / 2 - 1], &mut |_| {
            panic!("committed before pause threshold")
        })
        .unwrap();
    let mut emitted = false;
    segmenter
        .push(&[0.0], &mut |pcm| {
            assert_eq!(pcm.len(), SAMPLE_RATE * 3 / 2);
            emitted = true;
            Ok(())
        })
        .unwrap();
    assert!(emitted);
    segmenter
        .finish(&mut |_| panic!("finalization duplicated committed audio"))
        .unwrap();
}
