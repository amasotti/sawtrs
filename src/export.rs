use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
}

/// A segment to export — module-independent, no imports from store.
pub struct ExportSegment {
    pub index: usize,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Format segments as a console table.
pub fn format_table(video_id: &str, segments: &[ExportSegment]) -> comfy_table::Table {
    let mut table = comfy_table::Table::new();
    table.set_header(["#", "Video", "Start", "End", "Text"]);

    for seg in segments {
        table.add_row([
            (seg.index + 1).to_string(),
            video_id.to_string(),
            format_timestamp(seg.start),
            format_timestamp(seg.end),
            seg.text.clone(),
        ]);
    }

    table
}

/// Write segments to a CSV file.
pub fn write_csv(path: &str, segments: &[ExportSegment]) -> Result<(), ExportError> {
    let mut wtr = csv::Writer::from_path(Path::new(path))?;
    wtr.write_record(["start", "end", "text"])?;

    for seg in segments {
        wtr.write_record([
            &format_timestamp(seg.start),
            &format_timestamp(seg.end),
            &seg.text,
        ])?;
    }

    wtr.flush()?;
    Ok(())
}

/// Write segments as CSV to stdout.
pub fn write_csv_stdout(segments: &[ExportSegment]) -> Result<(), ExportError> {
    let stdout = std::io::stdout();
    let mut wtr = csv::Writer::from_writer(stdout.lock());
    wtr.write_record(["start", "end", "text"])?;

    for seg in segments {
        wtr.write_record([
            &format_timestamp(seg.start),
            &format_timestamp(seg.end),
            &seg.text,
        ])?;
    }

    wtr.flush()?;
    Ok(())
}

fn format_timestamp(seconds: f64) -> String {
    let mins = (seconds / 60.0) as u32;
    let secs = seconds % 60.0;
    format!("{mins:02}:{secs:05.2}")
}
