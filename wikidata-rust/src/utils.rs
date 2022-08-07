use indicatif::{ProgressBar, ProgressStyle};

pub fn new_progress_bar(len: u64) -> ProgressBar {
    let bar = ProgressBar::new(len);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40} {pos:>8}/{len:8} [{per_sec}] [ETA:{eta_precise}] {msg}"),
    );
    bar.set_draw_rate(1);
    return bar;
}