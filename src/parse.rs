use std::path::Path;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

use crate::{find_unused_bindings, remove::remove_unused, NoUnusedVarsOptions, Removal};

pub(crate) fn process_source(
    path: &Path,
    source: &str,
    options: &NoUnusedVarsOptions,
) -> Result<Option<(String, Vec<Removal>)>, String> {
    if should_skip_component_file(path) {
        return Ok(None);
    }
    let source_type =
        SourceType::from_path(path).map_err(|error| format!("unsupported source type: {error}"))?;
    if source_type.is_typescript_definition() {
        return Ok(None);
    }
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(format_diagnostics("parse", &parsed.diagnostics));
    }

    let semantic = SemanticBuilder::new_compiler()
        .with_build_nodes(true)
        .build(&parsed.program);
    if !semantic.diagnostics.is_empty() {
        return Err(format_diagnostics("semantic", &semantic.diagnostics));
    }

    let unused = find_unused_bindings(&semantic.semantic, &parsed.module_record, options);
    Ok(Some(remove_unused(
        source,
        &semantic.semantic,
        &unused,
        options,
    )))
}

fn should_skip_component_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("vue" | "svelte" | "astro")
    )
}

fn format_diagnostics(phase: &str, diagnostics: &[oxc_diagnostics::OxcDiagnostic]) -> String {
    let messages = diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    format!("{phase} failed: {messages}")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    use crate::NoUnusedVarsOptions;

    use super::process_source;

    #[test]
    fn skips_declaration_files() {
        for path in ["types.d.ts", "types.d.mts", "types.d.cts"] {
            assert!(process_source(
                Path::new(path),
                "declare const value: string",
                &NoUnusedVarsOptions::default()
            )
            .unwrap()
            .is_none());
        }
    }

    fn update(source: &str) -> String {
        let updated = process_source(
            Path::new("source.ts"),
            source,
            &NoUnusedVarsOptions::default(),
        )
        .unwrap()
        .unwrap()
        .0;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &updated, SourceType::ts()).parse();
        assert!(
            parsed.diagnostics.is_empty(),
            "transformed output did not parse: {:?}\n{updated}",
            parsed.diagnostics
        );
        updated
    }

    #[test]
    fn removes_a_multi_declarator_item() {
        assert_eq!(
            update("const used = 1, unused = 2;\nconsole.log(used);\n"),
            "const used = 1;\nconsole.log(used);\n"
        );
    }

    #[test]
    fn removes_a_trailing_parameter() {
        assert_eq!(
            update("function f(used, unused, also_unused) { return used; }\nf(1);\n"),
            "function f(used) { return used; }\nf(1);\n"
        );
    }

    #[test]
    fn removes_a_catch_binding() {
        assert_eq!(
            update("try {} catch (error) { console.log('x'); }\n"),
            "try {} catch { console.log('x'); }\n"
        );
    }

    #[test]
    fn removes_an_attached_leading_comment() {
        assert_eq!(
            update(
                "// attached\n/* multi-line\n * attached\n */\nconst unused = 1;\nconsole.log('x');\n"
            ),
            "console.log('x');\n"
        );
    }

    #[test]
    fn leaves_unused_imports_alone() {
        let source = "import value from './value.js';\nconsole.log('x');\n";
        assert_eq!(update(source), source);
    }

    #[test]
    fn keeps_export_ancestors_when_removing_nested_locals() {
        assert_eq!(
            update(
                "export function kept() { const unused = 1; return 1; }\nconsole.log(kept());\n"
            ),
            "export function kept() {  return 1; }\nconsole.log(kept());\n"
        );
        assert_eq!(
            update(
                "export class Kept { method() { const unused = 1; return 1; } }\nconsole.log(Kept);\n"
            ),
            "export class Kept { method() {  return 1; } }\nconsole.log(Kept);\n"
        );
        assert_eq!(
            update(
                "export const kept = () => { const unused = 1; return 1; };\nconsole.log(kept());\n"
            ),
            "export const kept = () => {  return 1; };\nconsole.log(kept());\n"
        );
    }

    #[test]
    fn removes_all_variable_declarators_as_one_statement() {
        assert_eq!(
            update("const first = 1, second = 2;\nconsole.log('x');\n"),
            "console.log('x');\n"
        );
        // Whole unused destructuring is skipped: evaluating the pattern can run getters/iterators.
        let destructured =
            "declare const value: { first: number, second: number };\nconst { first, second } = value;\nconsole.log('x');\n";
        assert_eq!(update(destructured), destructured);
    }

    #[test]
    fn removes_multiple_declarators_without_dangling_commas() {
        assert_eq!(
            update("const first = 1, second = 2, kept = 3;\nconsole.log(kept);\n"),
            "const kept = 3;\nconsole.log(kept);\n"
        );
        assert_eq!(
            update("const first = 1, kept = 2, third = 3;\nconsole.log(kept);\n"),
            "const kept = 2;\nconsole.log(kept);\n"
        );
    }

    #[test]
    fn skips_loop_header_declarations() {
        let for_of =
            "declare const values: number[];\nfor (const unused of values) { console.log('x'); }\n";
        assert_eq!(update(for_of), for_of);
        let for_in =
            "declare const values: object;\nfor (const unused in values) { console.log('x'); }\n";
        assert_eq!(update(for_in), for_in);
        let destructured =
            "declare const values: [number, number][];\nfor (const [unused, used] of values) { console.log(used); }\n";
        assert_eq!(update(destructured), destructured);
    }

    #[test]
    fn skips_destructured_parameters_and_unparenthesized_arrow_parameters() {
        let destructured =
            "function f({ unused, used }: { unused: number, used: number }) { return used; }\nf({ unused: 1, used: 2 });\n";
        assert_eq!(update(destructured), destructured);
        let arrow = "const f = x => 1;\nconsole.log(f());\n";
        assert_eq!(update(arrow), arrow);
        let bare_arrow = "x => 1;\n";
        assert_eq!(update(bare_arrow), bare_arrow);
    }

    #[test]
    fn skips_whole_declarations_used_as_single_statement_bodies() {
        let source = "declare const condition: boolean;\nif (condition) var unused = 1;\nconsole.log(condition);\n";
        assert_eq!(update(source), source);
    }

    #[test]
    fn skips_partial_destructuring_edits() {
        let array =
            "declare const pair: [number, number];\nconst [unused, used] = pair;\nconsole.log(used);\n";
        assert_eq!(update(array), array);
        let nested =
            "declare const value: { pair: [number, number] };\nconst { pair: [unused, used] } = value;\nconsole.log(used);\n";
        assert_eq!(update(nested), nested);
    }

    #[test]
    fn skips_destructured_catch_parameters() {
        let source = "try {} catch ({ unused }) { console.log('x'); }\n";
        assert_eq!(update(source), source);
    }

    #[test]
    fn skips_destructure_edits_across_comments() {
        let source =
            "declare const value: { unused: number, used: number };\nconst { unused /* keep */, used } = value;\nconsole.log(used);\n";
        assert_eq!(update(source), source);
        let preceding =
            "declare const value: { unused: number, used: number };\nconst { used, /* keep */ unused } = value;\nconsole.log(used);\n";
        assert_eq!(update(preceding), preceding);
        let all_unused =
            "declare const value: { first: number, second: number };\nconst { first /* keep */, second } = value;\nconsole.log('x');\n";
        assert_eq!(update(all_unused), all_unused);
    }

    #[test]
    fn skips_side_effectful_initializers() {
        let source = "const unused = console.log('effect');\nconsole.log('x');\n";
        assert_eq!(update(source), source);
    }

    #[test]
    fn skips_catch_bindings_with_comments() {
        let source = "try {} catch (/* keep */ error) { console.log('x'); }\n";
        assert_eq!(update(source), source);
    }

    #[test]
    fn removes_trailing_parameters_when_args_are_all() {
        let mut options = NoUnusedVarsOptions::default();
        options.args = crate::ArgsOption::All;
        let source = "function f(used, unused) { return used; }\nf(1);\n";
        let updated = process_source(Path::new("source.ts"), source, &options)
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(updated, "function f(used) { return used; }\nf(1);\n");
    }



    #[test]
    fn skips_write_only_parameters() {
        let mut options = NoUnusedVarsOptions::default();
        options.args = crate::ArgsOption::All;
        let source = "function f(unused) { unused = 1; }\nf(0);\n";
        let updated = process_source(Path::new("source.ts"), source, &options)
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(updated, source);
    }

    #[test]
    fn skips_effectful_unary_initializers() {
        let source = "const unused = !console.log('effect');\nconsole.log('x');\n";
        assert_eq!(update(source), source);
    }

    #[test]
    fn skips_effectful_parameter_defaults() {
        let mut options = NoUnusedVarsOptions::default();
        options.args = crate::ArgsOption::All;
        let source = "function f(unused = console.log('effect')) {}\nf();\n";
        let updated = process_source(Path::new("source.ts"), source, &options)
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(updated, source);
    }
}
