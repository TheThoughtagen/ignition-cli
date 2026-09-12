//! TAGS-12 loss-report advisory scans (11-03).
//!
//! Pure functions over raw bytes — no I/O, no gateway, no network. Transfer is
//! PASSTHROUGH: parsing exists ONLY to warn (the 11-04/11-05 consumers key on
//! [`codes`] and `top_level_names` — the scan output REPLACES a re-parse).
//!
//! # Advisory posture (planner lock)
//!
//! The scan NEVER refuses a file the gateway would accept. It returns a report
//! of what it saw — `partial: true` plus [`codes::XML_PARSE_PARTIAL`] when the
//! bytes cut short mid-parse — and hands the hard verdict to the gateway. An
//! unparseable file is the gateway's error to make, not the scan's.

use quick_xml::XmlVersion;
use quick_xml::events::BytesStart;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// Stable [`LossFact::code`] vocabulary — the report contract 11-05 renders
/// and 11-04/11-06 key on. Codes are never renamed, only added.
pub mod codes {
    /// XML file is gateway-export-shaped (MinVersion root attr): exports carry
    /// only EDITED properties, so re-import may differ from the live model.
    pub const XML_EXPORT_EDITED_ONLY: &str = "xml_export_edited_only";
    /// XML parse ended early (truncated/malformed input) — partial report.
    pub const XML_PARSE_PARTIAL: &str = "xml_parse_partial";
    /// File contains `type="UdtType"` definition(s): importTags refuses them
    /// ("Udt definitions can only be imported in the UDT Definitions tab") —
    /// the import lands nothing (11-01 probe 3, both rigs).
    pub const XML_UDT_TYPE_DEFINITION: &str = "xml_udt_type_definition";
    /// UNCONDITIONAL for non-empty CSV: the legacy format cannot carry alarm
    /// configurations — alarms never arrive on import (11-01 probe 5).
    pub const CSV_NO_ALARMS: &str = "csv_no_alarms";
    /// UNCONDITIONAL for non-empty CSV: legacy column vocabulary only — modern
    /// properties (tag groups, bindings, UDT parameter overrides) inexpressible.
    pub const CSV_LEGACY_COLUMNS_ONLY: &str = "csv_legacy_columns_only";
    /// A TagType/DataType/AccessRights cell carries a numeric enum value.
    pub const CSV_NUMERIC_ENUM: &str = "csv_numeric_enum";
    /// No `# version=N` marker row found — the gateway may reject the file.
    pub const CSV_MISSING_VERSION_MARKER: &str = "csv_missing_version_marker";
}

/// One advisory loss finding: a stable machine code (see [`codes`]) plus a
/// human-readable detail line (the report content 11-05 renders to stderr and
/// embeds as `data.loss_report`). `Serialize` lets facts ride the import
/// result envelope additively (11-04 `TagsImportResult.loss_facts`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LossFact {
    pub code: &'static str,
    pub detail: String,
}

/// What an advisory scan of a tag-exchange XML document saw: tallies and names
/// only — never parsed tag models (the parse authority is the gateway).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct XmlScan {
    pub tag_count: usize,
    pub types_seen: Vec<String>,
    pub has_compound_property: bool,
    pub top_level_names: Vec<String>,
    pub root_attrs: Vec<(String, String)>,
    pub partial: bool,
    pub facts: Vec<LossFact>,
}

/// What an advisory scan of a legacy CSV import saw: tallies and names only.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CsvScan {
    pub has_version_marker: bool,
    pub columns: Vec<String>,
    pub row_count: usize,
    pub top_level_names: Vec<String>,
    pub numeric_enum_count: usize,
    pub facts: Vec<LossFact>,
}

/// Advisory scan of a tag-exchange XML document (the gateway's export format).
///
/// Lenient by contract: a parse error sets `partial` + a
/// [`codes::XML_PARSE_PARTIAL`] fact and reports whatever was readable — it
/// never refuses. Deterministic; tallies and names only, no tag models.
pub fn scan_xml(raw: &[u8]) -> XmlScan {
    let mut reader = Reader::from_reader(raw);
    reader.config_mut().check_end_names = true;
    let mut buf = Vec::new();
    let mut scan = XmlScan::default();
    let mut depth: usize = 0;
    let mut saw_root = false;
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                if !saw_root {
                    saw_root = true;
                    scan.root_attrs = collect_attrs(e);
                } else if name.as_ref() == b"Tag"
                    && depth == 1
                    && let Some(n) = attr_value(e, b"name")
                {
                    push_unique(&mut scan.top_level_names, n);
                }
                match name.as_ref() {
                    b"Tag" => {
                        scan.tag_count += 1;
                        if let Some(t) = attr_value(e, b"type") {
                            push_unique(&mut scan.types_seen, t);
                        }
                    }
                    b"CompoundProperty" => scan.has_compound_property = true,
                    _ => {}
                }
                depth += 1;
            }
            Ok(Event::Empty(ref e)) => {
                let name = e.name();
                if !saw_root {
                    saw_root = true;
                    scan.root_attrs = collect_attrs(e);
                }
                match name.as_ref() {
                    b"Tag" => {
                        scan.tag_count += 1;
                        if let Some(t) = attr_value(e, b"type") {
                            push_unique(&mut scan.types_seen, t);
                        }
                        if depth == 1
                            && let Some(n) = attr_value(e, b"name")
                        {
                            push_unique(&mut scan.top_level_names, n);
                        }
                    }
                    b"CompoundProperty" => scan.has_compound_property = true,
                    _ => {}
                }
            }
            Ok(Event::End(_)) => {
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => break,
            Err(_) => {
                scan.partial = true;
                break;
            }
            _ => {}
        }
    }
    if scan.partial {
        scan.facts.push(LossFact {
            code: codes::XML_PARSE_PARTIAL,
            detail: format!(
                "XML parse ended early (malformed/truncated input); the report reflects the \
                 {} Tag element(s) readable before the error — the gateway makes the final call",
                scan.tag_count
            ),
        });
    }
    if let Some((_, min_version)) = scan
        .root_attrs
        .iter()
        .find(|(k, _)| k == "MinVersion")
        .map(|(k, v)| (k.clone(), v.clone()))
    {
        scan.facts.push(LossFact {
            code: codes::XML_EXPORT_EDITED_ONLY,
            detail: format!(
                "gateway-export-shaped file (MinVersion=\"{min_version}\"): exports carry ONLY \
                 edited properties — unset properties ride defaults or the referenced UDT type"
            ),
        });
    }
    if scan.types_seen.iter().any(|t| t == "UdtType") {
        scan.facts.push(LossFact {
            code: codes::XML_UDT_TYPE_DEFINITION,
            detail: "file contains UDT type definition(s) (type=\"UdtType\") — importTags \
                     refuses them (\"Udt definitions can only be imported in the UDT \
                     Definitions tab\"), so the import lands nothing"
                .to_string(),
        });
    }
    scan
}

/// Advisory scan of a legacy CSV import (the gateway's import-only format).
///
/// Lenient by contract: malformed records are skipped, never refused. The
/// `# version=N` marker may sit before OR after the header row (both shapes
/// seen in the wild); marker rows are excluded from `row_count`.
pub fn scan_csv(raw: &[u8]) -> CsvScan {
    let mut scan = CsvScan::default();
    if raw.iter().all(|&b| b.is_ascii_whitespace()) {
        return scan;
    }
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(raw);
    let mut records: Vec<Vec<String>> = Vec::new();
    for record in reader.records().flatten() {
        // Lenient: malformed records (bad UTF-8, unbalanced quote) are skipped
        // by the flatten above, never a refusal — the gateway owns the verdict.
        records.push(record.iter().map(|c| c.to_string()).collect());
    }
    let is_marker = |record: &[String]| {
        record
            .first()
            .map(|cell| {
                cell.trim()
                    .trim_start_matches('\u{feff}')
                    .starts_with("# version=")
            })
            .unwrap_or(false)
    };
    scan.has_version_marker = records.iter().any(|r| is_marker(r));
    let body: Vec<&Vec<String>> = records.iter().filter(|r| !is_marker(r)).collect();
    let columns: Vec<String> = body.first().map(|h| (*h).clone()).unwrap_or_default();
    scan.columns = columns.clone();
    let data: Vec<&&Vec<String>> = body.iter().skip(1).collect();
    scan.row_count = data.len();

    let idx_of = |name: &str| {
        columns.iter().position(|c| {
            c.trim()
                .trim_start_matches('\u{feff}')
                .eq_ignore_ascii_case(name)
        })
    };
    let name_idx = idx_of("name");
    let path_idx = idx_of("path");
    let enum_cols: Vec<usize> = ["tagtype", "datatype", "accessrights"]
        .iter()
        .filter_map(|c| idx_of(c))
        .collect();

    for row in &data {
        let name = name_idx
            .and_then(|i| row.get(i))
            .map(|s| s.trim())
            .unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let path = path_idx
            .and_then(|i| row.get(i))
            .map(|s| s.trim())
            .unwrap_or("");
        let top = if path.is_empty() {
            name.to_string()
        } else {
            match path.find('/') {
                Some(0) => continue,
                Some(i) => path[..i].to_string(),
                None => path.to_string(),
            }
        };
        if !top.is_empty() {
            push_unique(&mut scan.top_level_names, top);
        }
    }

    for (row_num, row) in data.iter().enumerate() {
        for &col_idx in &enum_cols {
            if let Some(cell) = row.get(col_idx) {
                let value = cell.trim();
                if value.parse::<i64>().is_ok() {
                    scan.numeric_enum_count += 1;
                    scan.facts.push(LossFact {
                        code: codes::CSV_NUMERIC_ENUM,
                        detail: format!(
                            "column {} carries numeric enum value {} (data row {})",
                            columns[col_idx].trim(),
                            value,
                            row_num + 1
                        ),
                    });
                }
            }
        }
    }

    if !records.is_empty() {
        scan.facts.push(LossFact {
            code: codes::CSV_NO_ALARMS,
            detail: "the legacy CSV format cannot carry alarm configurations — alarms never \
                     arrive on import (silently dropped even via the undocumented AlarmStates \
                     column)"
                .to_string(),
        });
        scan.facts.push(LossFact {
            code: codes::CSV_LEGACY_COLUMNS_ONLY,
            detail: "file uses the legacy column vocabulary — modern properties (tag groups, \
                     bindings, UDT parameter overrides) are not expressible in CSV"
                .to_string(),
        });
        if !scan.has_version_marker {
            scan.facts.push(LossFact {
                code: codes::CSV_MISSING_VERSION_MARKER,
                detail: "no '# version=N' marker row found — the gateway rejects marker-less \
                         CSV ('Unknown CSV Format')"
                    .to_string(),
            });
        }
    }
    scan
}

/// First attribute value with the given (byte) key, entity-normalized; `None`
/// when the attribute is absent or malformed — absence is never an error here.
/// Normalization assumes XML 1.0 (gateway exports carry no declaration).
fn attr_value(element: &BytesStart, key: &[u8]) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|attr| attr.key.as_ref() == key)
        .and_then(|attr| {
            attr.normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|v| v.into_owned())
        })
}

/// All root-element attributes in encounter order; malformed entries skipped.
fn collect_attrs(element: &BytesStart) -> Vec<(String, String)> {
    element
        .attributes()
        .flatten()
        .filter_map(|attr| {
            attr.normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|v| {
                    (
                        String::from_utf8_lossy(attr.key.as_ref()).into_owned(),
                        v.into_owned(),
                    )
                })
        })
        .collect()
}

/// Order-preserving dedupe push (report vocabulary, not a multiset).
fn push_unique(target: &mut Vec<String>, value: String) {
    if !target.iter().any(|t| t == &value) {
        target.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The XML sample the gateway actually produces — official docs, verbatim
    /// from 11-RESEARCH.md §Code Examples (AtomicTag with CompoundProperty
    /// alarms, parameter-bound property, MinVersion/locale root attrs).
    const DOCS_SAMPLE_XML: &str = r#"<Tags MinVersion="8.0.0" locale="en_US">
   <Tag name="Amps" type="AtomicTag">
      <Property name="opcItemPath" boundValueType="parameter">ns=1;s=[Dairy]_Meta:Overview/Motor {MotorNumber}/Amps</Property>
      <Property name="valueSource">opc</Property>
      <Property name="historyProvider" datatype="String">MySQL</Property>
      <CompoundProperty name="alarms">
         <PropertySet>
            <Property name="mode">3</Property>
            <Property name="setpointA">25</Property>
            <Property name="name">Low Amps</Property>
            <Property name="priority">4</Property>
            <Property name="displayPath" bindtype="Expression">Motor{MotorNumber}</Property>
         </PropertySet>
      </CompoundProperty>
      <Property name="historyEnabled" datatype="Boolean">true</Property>
   </Tag>
</Tags>"#;

    fn has_fact(facts: &[LossFact], code: &str) -> bool {
        facts.iter().any(|f| f.code == code)
    }

    // ------------------------------------------------------------------
    // scan_xml
    // ------------------------------------------------------------------

    #[test]
    fn scan_xml_docs_sample_counts_tags_types_alarms_and_names() {
        let scan = scan_xml(DOCS_SAMPLE_XML.as_bytes());
        assert_eq!(scan.tag_count, 1);
        assert_eq!(scan.types_seen, vec!["AtomicTag".to_string()]);
        assert!(scan.has_compound_property);
        assert_eq!(scan.top_level_names, vec!["Amps".to_string()]);
        assert!(
            scan.root_attrs
                .iter()
                .any(|(k, v)| k == "MinVersion" && v == "8.0.0")
        );
        assert!(
            scan.root_attrs
                .iter()
                .any(|(k, v)| k == "locale" && v == "en_US")
        );
        assert!(
            has_fact(&scan.facts, codes::XML_EXPORT_EDITED_ONLY),
            "MinVersion root attr must trigger the export-shape advisory; got {:?}",
            scan.facts
        );
        assert!(!scan.partial);
    }

    #[test]
    fn scan_xml_real_udt_capture_parses_clean_with_full_shape() {
        // THE anchor fixture: the REAL multi-level UDT XML captured from a live
        // 8.3.6 gateway in 11-01 (sha256 11ed1528…, byte-fidelity-preserved).
        // If the real bytes break the event loop, FIX THE SCAN, not the fixture.
        let raw = include_str!("../../tests/fixtures/udt-multilevel.xml");
        let scan = scan_xml(raw.as_bytes());
        assert!(
            !scan.partial,
            "the real capture must parse clean; got facts {:?}",
            scan.facts
        );
        assert_eq!(
            scan.tag_count, 8,
            "P11UDT+M2+Sub+M3+M1+MotorType+Doubled+Amps"
        );
        assert!(scan.has_compound_property, "alarms were seeded in 11-01");
        assert_eq!(
            scan.top_level_names,
            vec!["P11UDT".to_string(), "MotorType".to_string()]
        );
        for expected in ["Folder", "UdtInstance", "UdtType", "AtomicTag"] {
            assert!(
                scan.types_seen.iter().any(|t| t == expected),
                "types_seen must surface {expected}; got {:?}",
                scan.types_seen
            );
        }
        assert!(
            scan.root_attrs
                .iter()
                .any(|(k, v)| k == "MinVersion" && v == "8.0.0")
        );
        assert!(has_fact(&scan.facts, codes::XML_EXPORT_EDITED_ONLY));
        // 11-01 capture decision: the scan must surface UdtType presence —
        // importTags REFUSES type definitions and lands nothing.
        assert!(has_fact(&scan.facts, codes::XML_UDT_TYPE_DEFINITION));
    }

    #[test]
    fn scan_xml_truncated_input_reports_partial_without_error() {
        // One COMPLETE Tag element, then an open tag cut mid-header: quick-xml
        // never emits a Start event for an unterminated element header, so the
        // truncation error fires while reading `<Tag name="Y"` — the complete
        // Tag seen before the error still rides the report.
        const TRUNCATED: &[u8] =
            b"<Tags MinVersion=\"8.0.0\" locale=\"en_US\">\r\n   <Tag name=\"X\" type=\"AtomicTag\"/>\r\n   <Tag name=\"Y\"";
        let scan = scan_xml(TRUNCATED);
        assert!(scan.partial, "truncated input must flag partial");
        assert_eq!(
            scan.tag_count, 1,
            "the Tag seen before the error is reported"
        );
        assert!(has_fact(&scan.facts, codes::XML_PARSE_PARTIAL));
        assert!(
            scan.top_level_names.contains(&"X".to_string()),
            "whatever was readable before the error rides the report"
        );
        assert_eq!(
            scan.root_attrs.len(),
            2,
            "root attrs captured before the error"
        );
    }

    #[test]
    fn scan_xml_empty_input_is_zero_scan_no_panic() {
        let scan = scan_xml(b"");
        assert_eq!(scan.tag_count, 0);
        assert!(scan.types_seen.is_empty());
        assert!(!scan.has_compound_property);
        assert!(scan.top_level_names.is_empty());
        assert!(scan.root_attrs.is_empty());
        assert!(!scan.partial, "empty is not a parse error");
        assert!(scan.facts.is_empty());
    }

    // ------------------------------------------------------------------
    // scan_csv
    // ------------------------------------------------------------------

    /// The legacy CSV sample shape — header row verbatim from 11-RESEARCH.md
    /// §Code Examples (the documented legacy grammar: 48 columns), marker row,
    /// memory-tag row, folder row (folder membership rides the Path column).
    const DOCS_SAMPLE_CSV: &str = r#"Path,Name,Owner,TagType,DataType,Value,Enabled,AccessRights,OPCServer,OPCItemPath,ScanClass,DriverName,ScaleMode,RawLow,RawHigh,ScaledLow,ScaledHigh,ClampMode,ScaleFactor,Deadband,DeadbandMode,FormatString,EngUnit,EngLow,EngHigh,EngLimitMode,Tooltip,Documentation,ExpressionType,Expression,OPCWriteBackServer,OPCWriteBackItemPath,SQLBindingDatasource,HistoryEnabled,PrimaryHistoryProvider,HistoricalScanclass,HistoricalDeadband,HistoricalDeadbandMode,InterpolationMode,HistoryMaxAgeMode,HistoryMaxAge,HistoryTimestampSource,UDTParentType,PersistValue,SourceDataType,SourceTagPath,SQLBindingPollRate,Permissions
# version=1
,Memory Tag,,1,7,I'm a memory Tag,TRUE,Read_Write
P11Csv/,OPC in a folder,,0,2,,TRUE,Read_Write,Ignition OPC-UA Server,[devicename]folder/path
"#;

    #[test]
    fn scan_csv_docs_sample_marker_columns_rows_and_names() {
        let scan = scan_csv(DOCS_SAMPLE_CSV.as_bytes());
        assert!(scan.has_version_marker);
        // The documented legacy header vocabulary (research §Code Examples
        // skeleton: 48 columns — the docs cite the table as 47/48 across
        // versions; the verbatim asset wins).
        assert_eq!(scan.columns.len(), 48);
        for expected in [
            "Path",
            "Name",
            "TagType",
            "DataType",
            "AccessRights",
            "UDTParentType",
            "SourceTagPath",
        ] {
            assert!(
                scan.columns.iter().any(|c| c == expected),
                "header must contain {expected}; got {:?}",
                scan.columns
            );
        }
        assert_eq!(scan.row_count, 2, "data rows; marker row excluded");
        assert_eq!(
            scan.top_level_names,
            vec!["Memory Tag".to_string(), "P11Csv".to_string()],
            "empty Path ⇒ the tag itself; `P11Csv/` prefix ⇒ top-level P11Csv"
        );
        assert!(
            has_fact(&scan.facts, codes::CSV_NO_ALARMS),
            "csv_no_alarms is UNCONDITIONAL — alarms cannot arrive in CSV"
        );
        assert!(has_fact(&scan.facts, codes::CSV_LEGACY_COLUMNS_ONLY));
        assert_eq!(
            scan.numeric_enum_count, 4,
            "TagType 1/0 + DataType 7/2 are numeric enum coercions"
        );
    }

    #[test]
    fn scan_csv_numeric_enum_facts_name_column_and_value() {
        const NUMERIC: &str = "Path,Name,TagType,DataType,AccessRights\r\n,EnumTag,0,2,1\r\n";
        let scan = scan_csv(NUMERIC.as_bytes());
        assert_eq!(scan.numeric_enum_count, 3);
        let numeric_facts: Vec<&LossFact> = scan
            .facts
            .iter()
            .filter(|f| f.code == codes::CSV_NUMERIC_ENUM)
            .collect();
        assert_eq!(numeric_facts.len(), 3);
        for (column, value) in [("TagType", "0"), ("DataType", "2"), ("AccessRights", "1")] {
            assert!(
                numeric_facts
                    .iter()
                    .any(|f| f.detail.contains(column) && f.detail.contains(value)),
                "fact naming column {column} with raw value {value} missing; got {:?}",
                numeric_facts
            );
        }
    }

    #[test]
    fn scan_csv_missing_marker_row_is_flagged_advisory() {
        const NO_MARKER: &str = "Path,Name,TagType\r\n,T1,1\r\n";
        let scan = scan_csv(NO_MARKER.as_bytes());
        assert!(!scan.has_version_marker);
        assert_eq!(scan.row_count, 1);
        assert!(
            has_fact(&scan.facts, codes::CSV_MISSING_VERSION_MARKER),
            "the gateway may reject a marker-less file — advisory, never a refusal"
        );
    }

    #[test]
    fn scan_csv_embedded_newline_cell_keeps_row_intact() {
        // A quoted Expression cell containing a real newline — RFC-4180 quoting.
        // If hand-rolled parsing snuck in, this row would split and row_count
        // would break.
        const MULTILINE: &str =
            "Path,Name,TagType,DataType,Expression\r\n,MultiLine,1,7,\"line1\nline2\"\r\n";
        let scan = scan_csv(MULTILINE.as_bytes());
        assert_eq!(scan.row_count, 1);
        assert_eq!(scan.top_level_names, vec!["MultiLine".to_string()]);
    }

    #[test]
    fn scan_csv_empty_input_is_zero_scan_no_panic() {
        let scan = scan_csv(b"");
        assert!(!scan.has_version_marker);
        assert!(scan.columns.is_empty());
        assert_eq!(scan.row_count, 0);
        assert!(scan.top_level_names.is_empty());
        assert_eq!(scan.numeric_enum_count, 0);
        assert!(scan.facts.is_empty());
    }
}
