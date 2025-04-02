use super::*;

/// Test metric type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MetricType {
    /// Duration (milliseconds)
    Duration,
    /// Bytes value
    Bytes,
    /// Integer value
    Integer,
    /// Float value
    Float,
    /// Text value
    Text,
}

/// Test metric definition
#[derive(Debug, Clone)]
pub struct MetricDefinition {
    /// Metric name
    pub name: String,
    /// Metric type
    pub metric_type: MetricType,
    /// Metric description
    pub description: String,
}

/// Test report structure
pub struct Report {
    /// Report file path
    pub report_path: PathBuf,
    /// Metric definitions
    metrics: Vec<MetricDefinition>,
    /// File handle
    file: Option<File>,
    /// Buffer for rows
    buffer: Vec<String>,
    /// Current row being built
    current_row: HashMap<String, String>,
    /// Buffer flush threshold
    flush_threshold: usize,
}

/// Report error types
#[derive(Debug)]
pub enum ReportError {
    /// IO error
    Io(std::io::Error),
    /// Format error
    Format(String),
    /// Metric error
    Metric(String),
}

impl From<std::io::Error> for ReportError {
    fn from(err: std::io::Error) -> Self {
        ReportError::Io(err)
    }
}

impl std::fmt::Display for ReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReportError::Io(err) => write!(f, "IO error: {}", err),
            ReportError::Format(msg) => write!(f, "Format error: {}", msg),
            ReportError::Metric(msg) => write!(f, "Metric error: {}", msg),
        }
    }
}

impl std::error::Error for ReportError {}

impl Report {
    /// Create new report instance
    pub fn new(
        path: PathBuf,
        metrics: Vec<MetricDefinition>,
        flush_threshold: Option<usize>,
    ) -> Result<Self, ReportError> {
        let mut report = Self {
            report_path: path,
            metrics,
            file: None,
            buffer: Vec::new(),
            current_row: HashMap::new(),
            flush_threshold: flush_threshold.unwrap_or(10),
        };

        // Initialize report file
        report.initialize()?;

        Ok(report)
    }

    /// Initialize report file
    fn initialize(&mut self) -> Result<(), ReportError> {
        let headers: Vec<String> = self.metrics.iter().map(|m| m.name.clone()).collect();

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.report_path)?;

        let mut file = file;
        file.write_all(format!("{}\n", headers.join(";")).as_bytes())?;

        // Save file handle for subsequent use
        self.file = Some(file);

        Ok(())
    }

    /// Get file handle
    fn get_file(&mut self) -> Result<&mut File, ReportError> {
        if self.file.is_none() {
            let file = OpenOptions::new().append(true).open(&self.report_path)?;

            self.file = Some(file);
        }

        Ok(self.file.as_mut().unwrap())
    }

    /// Add duration metric by name
    pub fn add_duration(&mut self, name: &str, duration: Duration) -> Result<(), ReportError> {
        self.check_metric_name_and_type(name, MetricType::Duration)?;
        self.current_row
            .insert(name.to_string(), duration.as_millis().to_string());
        Ok(())
    }

    /// Add bytes metric by name
    pub fn add_bytes(&mut self, name: &str, value: u64) -> Result<(), ReportError> {
        self.check_metric_name_and_type(name, MetricType::Bytes)?;
        self.current_row.insert(name.to_string(), value.to_string());
        Ok(())
    }

    /// Add integer metric by name
    pub fn add_integer(&mut self, name: &str, value: u64) -> Result<(), ReportError> {
        self.check_metric_name_and_type(name, MetricType::Integer)?;
        self.current_row.insert(name.to_string(), value.to_string());
        Ok(())
    }

    /// Add float metric by name
    pub fn add_float(&mut self, name: &str, value: f64) -> Result<(), ReportError> {
        self.check_metric_name_and_type(name, MetricType::Float)?;
        self.current_row.insert(name.to_string(), value.to_string());
        Ok(())
    }

    /// Add text metric by name
    pub fn add_text(&mut self, name: &str, value: &str) -> Result<(), ReportError> {
        self.check_metric_name_and_type(name, MetricType::Text)?;
        self.current_row.insert(name.to_string(), value.to_string());
        Ok(())
    }

    /// Add arbitrary type metric by name (auto conversion)
    pub fn add_value<T: ToString>(&mut self, name: &str, value: T) -> Result<(), ReportError> {
        if !self.metrics.iter().any(|m| m.name == name) {
            return Err(ReportError::Metric(format!(
                "Metric name '{}' not found",
                name
            )));
        }

        self.current_row.insert(name.to_string(), value.to_string());
        Ok(())
    }

    /// Helper method to check if metric name exists and has correct type
    fn check_metric_name_and_type(
        &self,
        name: &str,
        expected_type: MetricType,
    ) -> Result<(), ReportError> {
        let metric = self.metrics.iter().find(|m| m.name == name);

        match metric {
            Some(m) if m.metric_type == expected_type => Ok(()),
            Some(m) => Err(ReportError::Metric(format!(
                "Metric '{}' type mismatch, expected {:?}, actual {:?}",
                name, expected_type, m.metric_type
            ))),
            None => Err(ReportError::Metric(format!(
                "Metric name '{}' not found",
                name
            ))),
        }
    }

    /// Complete current row and add to buffer
    pub fn end_row(&mut self) -> Result<(), ReportError> {
        // Ensure all metrics have values
        for metric in &self.metrics {
            if !self.current_row.contains_key(&metric.name) {
                self.current_row.insert(metric.name.clone(), String::new());
            }
        }

        // Create row in the same order as metrics
        let row = self
            .metrics
            .iter()
            .map(|m| self.current_row.get(&m.name).cloned().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(";");

        self.buffer.push(row);

        // Clear current row for next use
        self.current_row.clear();

        // If buffer reaches threshold, flush to file
        if self.buffer.len() >= self.flush_threshold {
            self.flush_buffer()?;
        }

        Ok(())
    }

    /// Write buffer content to file
    pub fn flush_buffer(&mut self) -> Result<(), ReportError> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        // Clone buffer contents to avoid borrow conflict
        let buffer_contents = self.buffer.clone();
        let file = self.get_file()?;

        for row in buffer_contents.iter() {
            file.write_all(format!("{}\n", row).as_bytes())?;
        }

        self.buffer.clear();

        Ok(())
    }

    /// Generate summary statistics
    pub fn generate_summary(&mut self) -> Result<String, ReportError> {
        // Ensure all data is written to file
        self.flush_buffer()?;

        // Read CSV file
        let content = std::fs::read_to_string(&self.report_path)?;
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() {
            return Err(ReportError::Format("CSV file is empty".to_string()));
        }

        // Parse headers
        let headers: Vec<&str> = lines[0].split(';').collect();

        // Prepare data structure
        let mut data: Vec<Vec<f64>> = vec![Vec::new(); headers.len()];

        // Parse data rows
        for line in lines.iter().skip(1) {
            let values: Vec<&str> = line.split(';').collect();
            for (i, value) in values.iter().enumerate() {
                if i >= headers.len() {
                    break;
                }

                if let Ok(num) = value.parse::<f64>() {
                    data[i].push(num);
                }
            }
        }

        #[derive(Tabled)]
        struct MetricSummary {
            #[tabled(rename = "Metric")]
            metric: String,
            #[tabled(rename = "Average")]
            average: String,
            #[tabled(rename = "Minimum")]
            minimum: String,
            #[tabled(rename = "Maximum")]
            maximum: String,
            #[tabled(rename = "Sample Count")]
            count: String,
        }

        let mut summaries = Vec::new();

        for (i, header) in headers.iter().enumerate() {
            if data[i].is_empty() {
                continue;
            }

            let values = &data[i];
            let avg = values.iter().sum::<f64>() / values.len() as f64;
            let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));

            // Format output based on metric type
            let metric_type = if i < self.metrics.len() {
                self.metrics[i].metric_type
            } else {
                MetricType::Float
            };

            let (avg_str, min_str, max_str) = match metric_type {
                MetricType::Duration => (
                    format!("{:.2} ms", avg),
                    format!("{:.2} ms", min),
                    format!("{:.2} ms", max),
                ),
                MetricType::Bytes => (
                    format!("{:.2} B", avg),
                    format!("{:.2} B", min),
                    format!("{:.2} B", max),
                ),
                MetricType::Integer | MetricType::Float => (
                    format!("{:.2}", avg),
                    format!("{:.2}", min),
                    format!("{:.2}", max),
                ),
                MetricType::Text => ("-".to_string(), "-".to_string(), "-".to_string()),
            };

            summaries.push(MetricSummary {
                metric: header.to_string(),
                average: avg_str,
                minimum: min_str,
                maximum: max_str,
                count: values.len().to_string(),
            });
        }

        // Create the table and style it
        let mut table = Table::new(summaries);
        table
            .with(Style::markdown())
            .with(Modify::new(Columns::new(1..)).with(Alignment::right()));

        // Generate the final Markdown output
        let mut summary = String::new();
        summary.push_str("# Performance Test Summary\n\n");
        summary.push_str(&table.to_string());

        Ok(summary)
    }

    /// Save summary statistics to file
    pub fn save_summary(&mut self, name: &str) -> Result<PathBuf, ReportError> {
        let summary = self.generate_summary()?;

        let summary_path = self
            .report_path
            .clone()
            .with_file_name(format!("{}.md", name));

        let mut file = std::fs::File::create(&summary_path)?;
        file.write_all(summary.as_bytes())?;

        Ok(summary_path)
    }

    /// Generate chart data
    pub fn generate_chart_data(&mut self) -> Result<PathBuf, ReportError> {
        // Ensure all data is written to file
        self.flush_buffer()?;

        // Read CSV file
        let content = std::fs::read_to_string(&self.report_path)?;
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() {
            return Err(ReportError::Format("CSV file is empty".to_string()));
        }

        // Parse headers
        let headers: Vec<&str> = lines[0].split(';').collect();

        // Prepare data structure
        let mut data: Vec<Vec<f64>> = vec![Vec::new(); headers.len()];

        // Parse data rows
        for line in lines.iter().skip(1) {
            let values: Vec<&str> = line.split(';').collect();
            for (i, value) in values.iter().enumerate() {
                if i >= headers.len() {
                    break;
                }

                if let Ok(num) = value.parse::<f64>() {
                    data[i].push(num);
                }
            }
        }

        // Generate JSON
        let mut json = String::from("{\n");

        for (i, header) in headers.iter().enumerate() {
            if i > 0 {
                json.push_str(",\n");
            }

            json.push_str(&format!("  \"{}\": [", header));

            for (j, value) in data[i].iter().enumerate() {
                if j > 0 {
                    json.push_str(", ");
                }
                json.push_str(&format!("{}", value));
            }

            json.push(']');
        }

        json.push_str("\n}");

        // Save JSON
        let mut chart_path = self.report_path.clone();
        chart_path.set_extension("json");

        let mut file = std::fs::File::create(&chart_path)?;
        file.write_all(json.as_bytes())?;

        Ok(chart_path)
    }

    /// Ensure all data is written on drop
    pub fn finalize(&mut self) -> Result<(), ReportError> {
        self.flush_buffer()
    }

    /// For backward compatibility
    pub fn write_header(&self, _fields: &[&str]) {
        // Do nothing, headers are now defined by metrics
    }

    /// For backward compatibility
    pub fn write_duration(&self, _duration: Duration) {
        // Do nothing, use add_duration instead
    }

    /// For backward compatibility
    pub fn write_value<T: ToString>(&self, _value: T) {
        // Do nothing, use add_value instead
    }

    /// For backward compatibility
    pub fn end_line(&self) {
        // Do nothing, use end_row instead
    }
}

impl Drop for Report {
    fn drop(&mut self) {
        // Try to flush buffer, ignore errors
        let _ = self.flush_buffer();
    }
}
