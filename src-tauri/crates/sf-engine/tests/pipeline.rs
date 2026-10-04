//! End-to-end pipeline test: plan → execute → verify layout + history →
//! undo → verify restoration. Runs against a tempdir; no real user data.

use sf_engine::history;
use sf_engine::mover::{self, AskResolution};
use sf_engine::plan_inputs;
use sf_engine::types::{PlanMode, PlanStatus, Settings};
use std::fs;
use std::path::Path;

fn settings(sf: &Path) -> Settings {
    Settings {
        organize_root: Some(sf.to_path_buf()),
        ..Settings::default()
    }
}

#[test]
fn plan_execute_undo_full_cycle() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let sf = root.join("DeskZero");
    fs::create_dir_all(&sf).unwrap();
    let hist = root.join("data").join("history.jsonl");
    fs::create_dir_all(hist.parent().unwrap()).unwrap();

    // The spec's example drop.
    let inputs: Vec<std::path::PathBuf> = [
        ("photo.jpg", b"jpg" as &[u8]),
        ("invoice.pdf", b"pdf"),
        ("movie.mp4", b"mp4"),
        ("song.mp3", b"mp3"),
        ("backup.zip", b"zip"),
        ("installer.exe", b"exe"),
        ("document.docx", b"docx"),
        ("spreadsheet.xlsx", b"xlsx"),
    ]
    .iter()
    .map(|(name, bytes)| {
        let p = root.join(name);
        fs::write(&p, bytes).unwrap();
        p
    })
    .collect();

    let s = settings(&sf);
    let plan = plan_inputs(&inputs, &[], &s, PlanMode::OrganizeRoot);
    assert_eq!(plan.len(), 8);
    assert!(plan
        .iter()
        .all(|op| matches!(op.status, PlanStatus::Move { replace: false })));

    let mut counter = 0u64;
    let result = mover::execute(
        &plan,
        &hist,
        &root.join("data").join("quarantine"),
        &mut counter,
        AskResolution::Skip,
    )
    .unwrap();

    assert!(result.entry.failed.is_empty());
    assert!(sf.join("Images").join("photo.jpg").exists());
    assert!(sf.join("Documents").join("invoice.pdf").exists());
    assert!(sf.join("Documents").join("document.docx").exists());
    assert!(sf.join("Videos").join("movie.mp4").exists());
    assert!(sf.join("Audio").join("song.mp3").exists());
    assert!(sf.join("Archives").join("backup.zip").exists());
    assert!(sf.join("Installers").join("installer.exe").exists());
    assert!(sf.join("Spreadsheets").join("spreadsheet.xlsx").exists());
    assert!(inputs.iter().all(|p| !p.exists()));

    // History recorded one batch with all 8 items.
    let entries = history::read(&hist);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].items.len(), 8);

    // Undo puts everything back.
    mover::undo(&result.entry, &hist, &mut counter).unwrap();
    for p in &inputs {
        assert!(p.exists(), "{} must be restored", p.display());
    }
    assert!(!sf.join("Images").join("photo.jpg").exists());
    // Undo itself is recorded.
    let entries = history::read(&hist);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].kind, history::HistoryKind::Undo);
    assert_eq!(entries[0].items.len(), 8);
}

#[test]
fn conflict_rename_keeps_both_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let sf = root.join("DeskZero");
    let docs = sf.join("Documents");
    fs::create_dir_all(&docs).unwrap();
    fs::write(docs.join("report.pdf"), b"OLD").unwrap();
    let incoming = root.join("report.pdf");
    fs::write(&incoming, b"NEW").unwrap();

    let s = settings(&sf);
    let plan = plan_inputs(
        std::slice::from_ref(&incoming),
        &[],
        &s,
        PlanMode::OrganizeRoot,
    );
    assert_eq!(plan[0].dst.file_name().unwrap(), "report (1).pdf");

    let mut counter = 0u64;
    let result = mover::execute(
        &plan,
        &root.join("h.jsonl"),
        &root.join("q"),
        &mut counter,
        AskResolution::Skip,
    )
    .unwrap();
    assert!(result.entry.failed.is_empty());
    assert_eq!(fs::read(docs.join("report.pdf")).unwrap(), b"OLD");
    assert_eq!(fs::read(docs.join("report (1).pdf")).unwrap(), b"NEW");
}

#[test]
fn failed_move_is_recorded_and_batch_completes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let sf = root.join("DeskZero");
    fs::create_dir_all(&sf).unwrap();
    let ok = root.join("ok.jpg");
    fs::write(&ok, b"img").unwrap();
    let ghost = root.join("ghost.png");
    fs::write(&ghost, b"png").unwrap();

    let s = settings(&sf);
    let plan = plan_inputs(
        &[ok.clone(), ghost.clone()],
        &[],
        &s,
        PlanMode::OrganizeRoot,
    );
    assert_eq!(plan.len(), 2);
    fs::remove_file(&ghost).unwrap(); // simulate vanished file after planning

    let mut counter = 0u64;
    let result = mover::execute(
        &plan,
        &root.join("h.jsonl"),
        &root.join("q"),
        &mut counter,
        AskResolution::Skip,
    )
    .unwrap();

    assert!(
        sf.join("Images").join("ok.jpg").exists(),
        "good move must land"
    );
    assert_eq!(result.entry.items.len(), 1);
    assert_eq!(result.entry.failed.len(), 1);
    assert!(result.entry.failed[0].src.ends_with("ghost.png"));
}

#[test]
fn unicode_names_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let sf = root.join("DeskZero");
    fs::create_dir_all(&sf).unwrap();
    let file = root.join("facture été 2026.pdf");
    fs::write(&file, b"data").unwrap();

    let s = settings(&sf);
    let plan = plan_inputs(std::slice::from_ref(&file), &[], &s, PlanMode::OrganizeRoot);
    let mut counter = 0u64;
    let result = mover::execute(
        &plan,
        &root.join("h.jsonl"),
        &root.join("q"),
        &mut counter,
        AskResolution::Skip,
    )
    .unwrap();
    assert!(result.entry.failed.is_empty());
    assert!(sf.join("Documents").join("facture été 2026.pdf").exists());

    mover::undo(&result.entry, &root.join("h.jsonl"), &mut counter).unwrap();
    assert!(file.exists());
}

#[cfg(unix)]
#[test]
fn symlinks_move_as_links_not_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let sf = root.join("DeskZero");
    fs::create_dir_all(&sf).unwrap();
    let target = root.join("real.txt");
    fs::write(&target, b"data").unwrap();
    let link = root.join("link.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let s = settings(&sf);
    let plan = plan_inputs(std::slice::from_ref(&link), &[], &s, PlanMode::OrganizeRoot);
    let mut counter = 0u64;
    mover::execute(
        &plan,
        &root.join("h.jsonl"),
        &root.join("q"),
        &mut counter,
        AskResolution::Skip,
    )
    .unwrap();

    let moved = sf.join("Documents").join("link.txt");
    assert!(moved.exists());
    assert!(moved.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(target.exists(), "target must be untouched");
}
