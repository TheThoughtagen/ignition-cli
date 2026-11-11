# Native tag export fixtures

Each folder is a minimal project repo in the Ignition git module's native tag export format, the layout `ign deploy projects` reads (`tags/<provider>/...`). The tests in `crates/ignition-core/src/deploy/native_tags.rs` load them with `fixture("<folder>")`. Names are neutral (`PlantA`, `Line1`).

The odd-looking file names and formatting are deliberate test inputs. Do not "fix" them. `.gitattributes` marks this tree `-text`, so git never rewrites line endings.

| Folder | What it is | Test |
|---|---|---|
| `good/` | A valid export: provider `PlantA` with two UDT types (`_types_/Motor.json`, `_types_/Valve.json`), its tag groups (`.tag-groups.json`), the tag provider config (`tags/.tag-config.json`), a plain tag (`Line1/Speed.json`) and a percent-encoded name (`Line1/Motor%3AA.json` is the tag `Motor:A`). The reader must read it without findings. | `read_provider`, `read_tag_config` and `report` on `good` |
| `legacy-file/` | The older single-file layout `tags/PlantA.json`. It must be refused, naming the file and asking for individual files. | the legacy-layout refusal test |
| `legacy-dir/` | The older layout with one file inside the provider folder, `tags/PlantA/PlantA.json`. It is refused the same way. | the legacy-layout refusal test |
| `noncanonical/` | One file per formatting defect the export check reports, each with exactly one fixed reason. `Canonical.json` is the control and has no finding. | `each_defect_has_one_fixed_reason` |

The `noncanonical/tags/PlantA/` files and the reason each one reports:

| File | Reason |
|---|---|
| `Canonical.json` | none (control) |
| `Crlf.json` | CRLF line ending |
| `Escaping.json` | escaping |
| `Indent.json` | indent |
| `KeyOrder.json` | key order |
| `TrailingNewline.json` | trailing newline |
| `ValueOnly.json` | value without defaultValue |
