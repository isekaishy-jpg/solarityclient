//! Retained EditBox cluster geometry used by pointer cursor placement.

/// One EditBox's exact shaped line and UTF-8 cluster positions.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EditBoxTextLayout {
    text_length: usize,
    cursor: usize,
    selection: [usize; 2],
    lines: Vec<EditBoxLine>,
    clusters: Vec<EditBoxCluster>,
}

impl EditBoxTextLayout {
    /// Starts one layout from the live byte-indexed editing state.
    pub(crate) fn new(text_length: usize, cursor: usize, selection: [usize; 2]) -> Self {
        Self {
            text_length,
            cursor,
            selection,
            lines: Vec::new(),
            clusters: Vec::new(),
        }
    }

    /// Reserves the exact number of shaped lines before layout publication.
    pub(crate) fn reserve_lines(&mut self, count: usize) {
        self.lines.reserve(count);
    }

    /// Retains one baseline in owner-local top-origin coordinates.
    pub(crate) fn push_line(&mut self, baseline: f64) {
        self.lines.push(EditBoxLine { baseline });
    }

    /// Retains one visible scalar's source span and horizontal advance.
    pub(crate) fn push_cluster(
        &mut self,
        source_begin: usize,
        source_end: usize,
        line: usize,
        begin: f64,
        end: f64,
    ) {
        self.clusters.push(EditBoxCluster {
            source_begin,
            source_end,
            line,
            begin,
            end,
        });
    }

    /// Maps owner-local coordinates to the nearest byte insertion boundary.
    ///
    /// SolCL's retained input path selects the nearest line by baseline, then
    /// chooses each cluster's leading or trailing byte boundary at its
    /// horizontal midpoint. Empty lines resolve to the end of the buffer.
    pub(crate) fn cursor_at(&self, point: (f64, f64)) -> Option<usize> {
        let line = self
            .lines
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                (point.1 - left.baseline)
                    .abs()
                    .total_cmp(&(point.1 - right.baseline).abs())
            })
            .map(|(index, _)| index)?;
        let mut cursor = self.text_length;
        let mut found_cluster = false;
        for cluster in self.clusters.iter().filter(|cluster| cluster.line == line) {
            found_cluster = true;
            if point.0 < (cluster.begin + cluster.end) * 0.5 {
                cursor = cluster.source_begin;
                break;
            }
            cursor = cluster.source_end;
        }
        Some(if found_cluster {
            cursor
        } else {
            self.text_length
        })
    }

    /// Returns the stable endpoint opposite the live insertion cursor.
    pub(crate) fn selection_anchor(&self) -> usize {
        let [start, end] = self.selection;
        if start == end {
            self.cursor
        } else if self.cursor == start {
            end
        } else {
            start
        }
    }
}

/// Vertical position of one EditBox text line in owner-local coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
struct EditBoxLine {
    baseline: f64,
}

/// Horizontal advance and source span of one visible EditBox scalar.
#[derive(Clone, Copy, Debug, PartialEq)]
struct EditBoxCluster {
    source_begin: usize,
    source_end: usize,
    line: usize,
    begin: f64,
    end: f64,
}
