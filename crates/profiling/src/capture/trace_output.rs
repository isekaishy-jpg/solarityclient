//! Trace serialization stays on the writer; names contain only bounded asset paths.

use crate::trace::TraceRecord;
use std::io::{self, Write};

/// Declares the stable operation, provenance and workload columns.
pub(super) fn header(output: &mut impl Write) -> io::Result<()> {
    writeln!(
        output,
        "thread,span_id,parent_id,related_id,origin_frame,completion_frame,start_ns,duration_ns,kind,label,owner,reason,value,name"
    )
}

/// Rows use a shared monotonic CPU clock, unlike writer-interval event timestamps.
pub(super) fn row(output: &mut impl Write, thread: &str, row: TraceRecord) -> io::Result<()> {
    writeln!(
        output,
        "\"{}\",{},{},{},{},{},{},{},{},\"{}\",{},{},{},\"{}\"",
        thread.replace('"', "\"\""),
        row.id,
        row.parent,
        row.related,
        row.origin_frame,
        row.completion_frame,
        row.started_ns,
        row.duration_ns,
        row.kind,
        row.label.replace('"', "\"\""),
        row.owner,
        row.reason,
        row.value,
        row.name.as_deref().unwrap_or("").replace('"', "\"\"")
    )
}
