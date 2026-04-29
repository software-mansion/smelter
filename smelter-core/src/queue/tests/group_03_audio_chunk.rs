//! Group 3 — Audio chunking.
//!
//! Verification convention: identity is read from each batch's tagged first
//! sample; `start_pts` is asserted *after subtracting the known offset +
//! side_channel_delay* to recover the original input PTS. With offset Pts(0)
//! and side_channel_delay = 0, output start_pts == input start_pts.

use std::time::Duration;

use smelter_render::Framerate;

use crate::queue::{
    queue_input::{QueueInputOptions, QueueTrackOffset},
    tests::harness::{QueueHarness, read_audio_tag},
};

const FPS_30: Framerate = Framerate { num: 30, den: 1 };
const ZERO: Duration = Duration::ZERO;
const SAMPLE_RATE: u32 = 48000;

fn build() -> QueueHarness {
    QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(true)
        .build()
}

fn add_required(h: &QueueHarness, name: &str) -> crate::queue::tests::harness::InputHandle {
    h.add_input(
        name,
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    )
}

// ---------------------------------------------------------------------------
// 17. Default 20ms grid, offset Pts(0). Each chunk holds exactly one tagged
//     batch matching the chunk index.
// ---------------------------------------------------------------------------
#[test]
fn chunk_grid_one_batch_per_chunk() {
    let h = build();
    let a = add_required(&h, "a");
    a.queue_audio_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    let batch_len = Duration::from_millis(20);
    a.push_audio_stream(batch_len, 20, ZERO);
    a.drop_audio();
    h.advance(Duration::from_millis(500));
    h.wait_for_audio_count(20);
    let r = h.recorded();
    for i in 0..20 {
        let tags = r.audio_tags_for_input(i, &a.input_id);
        assert_eq!(tags, vec![(a.input_idx, i as u32)], "chunk {i} tag");
        let chunk = &r.audio[i];
        assert_eq!(chunk.start_pts, batch_len * i as u32, "chunk {i} start_pts");
        assert_eq!(
            chunk.end_pts,
            batch_len * (i as u32 + 1),
            "chunk {i} end_pts"
        );
    }
}

// ---------------------------------------------------------------------------
// 18. Input 10ms batches → 20ms chunks. Each chunk has 2 batches.
// ---------------------------------------------------------------------------
#[test]
fn chunks_with_two_batches_each() {
    let h = build();
    let a = add_required(&h, "a");
    a.queue_audio_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    let batch_len = Duration::from_millis(10);
    a.push_audio_stream(batch_len, 40, ZERO); // 40 batches × 10ms = 400ms = 20 chunks
    a.drop_audio();
    h.advance(Duration::from_millis(800));
    h.wait_for_audio_count(20);
    let r = h.recorded();
    for i in 0..20 {
        let tags = r.audio_tags_for_input(i, &a.input_id);
        assert_eq!(
            tags,
            vec![(a.input_idx, 2 * i as u32), (a.input_idx, 2 * i as u32 + 1)],
            "chunk {i} expected batches {} and {}",
            2 * i,
            2 * i + 1
        );
    }
}

// ---------------------------------------------------------------------------
// 19. Input 50ms batches → 20ms chunks. Each batch is popped exactly once,
//     in the first chunk whose end_pts > batch.start_pts.
// ---------------------------------------------------------------------------
#[test]
fn chunks_with_long_batches() {
    let h = build();
    let a = add_required(&h, "a");
    a.queue_audio_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    let batch_len = Duration::from_millis(50);
    a.push_audio_stream(batch_len, 4, ZERO); // 4 batches × 50ms = 200ms = 10 chunks
    a.drop_audio();
    h.advance(Duration::from_millis(500));
    h.wait_for_audio_count(10);
    let r = h.recorded();

    // Batch 0 starts at 0ms — first chunk where end_pts (20) > 0 is chunk 0.
    // Batch 1 starts at 50ms — first chunk where end_pts > 50 is chunk 2 (end=60).
    // Batch 2 starts at 100ms — first chunk where end_pts > 100 is chunk 5.
    // Batch 3 starts at 150ms — first chunk where end_pts > 150 is chunk 7.
    let expected: [(usize, u32); 4] = [(0, 0), (2, 1), (5, 2), (7, 3)];
    for (chunk_idx, seq) in expected {
        let tags = r.audio_tags_for_input(chunk_idx, &a.input_id);
        assert_eq!(
            tags,
            vec![(a.input_idx, seq)],
            "chunk {chunk_idx} should hold batch seq {seq}"
        );
    }
    // Other chunks should have empty samples (or absent if no input batch overlapped).
    let occupied: std::collections::HashSet<usize> =
        expected.iter().map(|&(c, _)| c).collect();
    for i in 0..10 {
        if occupied.contains(&i) {
            continue;
        }
        let tags = r.audio_tags_for_input(i, &a.input_id);
        assert!(
            tags.is_empty(),
            "chunk {i} expected empty (got {tags:?})"
        );
    }
}

// ---------------------------------------------------------------------------
// 20. Irregular batch sizes. Tag set across all chunks equals tag set across
//     all input batches.
// ---------------------------------------------------------------------------
#[test]
fn irregular_batch_sizes_invariant() {
    let h = build();
    let a = add_required(&h, "a");
    a.queue_audio_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    let lens: [u64; 6] = [7, 13, 9, 31, 5, 15]; // ms
    let mut start = ZERO;
    for (i, &len_ms) in lens.iter().enumerate() {
        let len = Duration::from_millis(len_ms);
        a.push_audio_batch(i as u32, start, len, SAMPLE_RATE);
        start += len;
    }
    a.drop_audio();
    h.advance(Duration::from_millis(300));
    h.wait_for_audio_count(5); // total 80ms / 20ms = 4 chunks; allow extras
    let r = h.recorded();

    // Collect every tag we see across all audio chunks.
    let mut seen: Vec<(u32, u32)> = Vec::new();
    for chunk in &r.audio {
        if let Some(crate::prelude::PipelineEvent::Data(batches)) =
            chunk.samples.get(&a.input_id)
        {
            for b in batches {
                seen.push(read_audio_tag(b));
            }
        }
    }
    seen.sort();
    seen.dedup();
    let expected: Vec<(u32, u32)> = (0..6).map(|i| (a.input_idx, i)).collect();
    assert_eq!(seen, expected, "every input batch should appear once");
}

// ---------------------------------------------------------------------------
// 21. No drift over many chunks: 100 contiguous 20ms batches → chunk 99 has
//     end_pts exactly 2000ms.
// ---------------------------------------------------------------------------
#[test]
fn chunk_pts_no_drift() {
    let h = build();
    let a = add_required(&h, "a");
    a.queue_audio_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    let batch_len = Duration::from_millis(20);
    a.push_audio_stream(batch_len, 100, ZERO);
    a.drop_audio();
    h.advance(Duration::from_millis(2500));
    h.wait_for_audio_count(100);
    let r = h.recorded();
    assert_eq!(r.audio[99].end_pts, Duration::from_millis(2000));
    assert_eq!(r.audio[99].start_pts, Duration::from_millis(1980));
}

// ---------------------------------------------------------------------------
// 22. Offset application visible in output start_pts.
// ---------------------------------------------------------------------------
#[test]
fn offset_visible_in_output_start_pts() {
    let h = build();
    let a = add_required(&h, "a");
    let offset = Duration::from_millis(500);
    a.queue_audio_track(QueueTrackOffset::Pts(offset));
    h.start();
    let batch_len = Duration::from_millis(20);
    a.push_audio_stream(batch_len, 5, ZERO);
    a.drop_audio();
    h.advance(Duration::from_millis(1500));
    h.wait_for_audio_count(30);
    let r = h.recorded();

    // Each input batch start_pts of N*20ms should surface as output start_pts
    // of N*20ms + 500ms (offset). Find the chunks where our tags appear.
    for input_seq in 0..5u32 {
        let mut found = None;
        for (i, chunk) in r.audio.iter().enumerate() {
            if let Some(crate::prelude::PipelineEvent::Data(batches)) =
                chunk.samples.get(&a.input_id)
            {
                for b in batches {
                    if read_audio_tag(b) == (a.input_idx, input_seq) {
                        // The batch's own start_pts (post-queue-rewrite)
                        // should equal input_pts + offset.
                        let expected =
                            offset + Duration::from_millis(20) * input_seq;
                        assert_eq!(
                            b.start_pts, expected,
                            "input seq {input_seq} batch start_pts after offset"
                        );
                        found = Some(i);
                        break;
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }
        assert!(
            found.is_some(),
            "input seq {input_seq} should surface in some chunk"
        );
    }
}
