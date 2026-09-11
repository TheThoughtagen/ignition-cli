//! TAGS-12 loss-report advisory scans (11-03).
//!
//! Pure functions over raw bytes — no I/O, no gateway, no network. Transfer is
//! PASSTHROUGH: parsing exists ONLY to warn. The scan is lenient by contract —
//! it never refuses a file the gateway would accept; partial knowledge rides
//! the report as [`LossFact`] entries, and hard refusals are the gateway's job.

/// One advisory loss finding: a stable machine code plus a human-readable
/// detail line (the report content 11-05 renders to stderr and embeds as
/// `data.loss_report`).
#[derive(Debug, Clone, PartialEq)]
pub struct LossFact {
    pub code: &'static str,
    pub detail: String,
}

/// What an advisory scan of a tag-exchange XML document saw.
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

/// What an advisory scan of a legacy CSV import saw.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CsvScan {
    pub has_version_marker: bool,
    pub columns: Vec<String>,
    pub row_count: usize,
    pub top_level_names: Vec<String>,
    pub numeric_enum_count: usize,
    pub facts: Vec<LossFact>,
}

pub fn scan_xml(_raw: &[u8]) -> XmlScan {
    XmlScan::default()
}

pub fn scan_csv(_raw: &[u8]) -> CsvScan {
    CsvScan::default()
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
        assert!(scan
            .root_attrs
            .iter()
            .any(|(k, v)| k == "MinVersion" && v == "8.0.0"));
        assert!(scan
            .root_attrs
            .iter()
            .any(|(k, v)| k == "locale" && v == "en_US"));
        assert!(
            has_fact(&scan.facts, "xml_export_edited_only"),
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
        assert_eq!(scan.tag_count, 8, "P11UDT+M2+Sub+M3+M1+MotorType+Doubled+Amps");
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
        assert!(scan
            .root_attrs
            .iter()
            .any(|(k, v)| k == "MinVersion" && v == "8.0.0"));
        assert!(has_fact(&scan.facts, "xml_export_edited_only"));
        // 11-01 capture decision: the scan must surface UdtType presence —
        // importTags REFUSES type definitions and lands nothing.
        assert!(has_fact(&scan.facts, "xml_udt_type_definition"));
    }

    #[test]
    fn scan_xml_truncated_input_reports_partial_without_error() {
        const TRUNCATED: &[u8] = b"<Tags MinVersion=\"8.0.0\" locale=\"en_US\">\r\n   <Tag name=\"X\"";
        let scan = scan_xml(TRUNCATED);
        assert!(scan.partial, "truncated input must flag partial");
        assert_eq!(scan.tag_count, 1, "the Tag seen before the error is reported");
        assert!(has_fact(&scan.facts, "xml_parse_partial"));
        assert!(
            scan.top_level_names.contains(&"X".to_string()),
            "whatever was readable before the error rides the report"
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
            has_fact(&scan.facts, "csv_no_alarms"),
            "csv_no_alarms is UNCONDITIONAL — alarms cannot arrive in CSV"
        );
        assert!(has_fact(&scan.facts, "csv_legacy_columns_only"));
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
            .filter(|f| f.code == "csv_numeric_enum")
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
            has_fact(&scan.facts, "csv_missing_version_marker"),
            "the gateway may reject a marker-less file — advisory, never a refusal"
        );
    }

    #[test]
    fn scan_csv_embedded_newline_cell_keeps_row_intact() {
        // A quoted Expression cell containing a real newline — RFC-4180 quoting.
        // If hand-rolled parsing snuck in, this row would split and row_count
        // would break.
        const MULTILINE: &str = "Path,Name,TagType,DataType,Expression\r\n,MultiLine,1,7,\"line1\nline2\"\r\n";
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
