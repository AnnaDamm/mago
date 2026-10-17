#![allow(clippy::expect_used, clippy::missing_panics_doc)]

use std::borrow::Cow;
use std::sync::LazyLock;

use mago_allocator::LocalArena;
use mago_analyzer::Analyzer;
use mago_analyzer::analysis_result::AnalysisResult;
use mago_analyzer::plugin::PluginRegistry;
use mago_analyzer::settings::Settings;
use mago_codex::populator::populate_codebase;
use mago_codex::scanner::scan_program;
use mago_database::file::File;
use mago_database::file::FileType;
use mago_names::resolver::NameResolver;
use mago_php_version::PHPVersion;
use mago_prelude::Prelude;
use mago_syntax::parser::parse_file;
use mago_word::WordSet;

static PRELUDE: LazyLock<Prelude> = LazyLock::new(Prelude::build);

const HELPERS: &str = r"<?php

namespace App;

use function \compact as snapshot;

function invoke_now(\Closure $callback): void
{
    $callback();
}

function invoke_after_compacting_other_variable(\Closure $callback): void
{
    compact('other');
    $callback();
}

function invoke_after_compacting_known_other_variable(\Closure $callback): void
{
    $name = 'other';
    compact($name);
    $callback();
}

function invoke_after_other_variable_variable(\Closure $callback): void
{
    $other = 'other';
    $$other = 'ignored';
    $callback();
}

function invoke_after_isset_other_variable(\Closure $callback): void
{
    $name = 'other';
    isset($name);
    $$name = 'ignored';
    $callback();
}

function mutate_variable_name(string &$name): void
{
    $name = 'callback';
}

function invoke_after_mutating_variable_name(\Closure $callback): void
{
    $name = 'other';
    mutate_variable_name($name);
    $GLOBALS['callbacks'][] = $$name;
    $callback();
}

function yield_then_invoke(\Closure $callback): \Generator
{
    yield $callback();
}

function store_then_invoke(\Closure $callback): void
{
    $GLOBALS['callbacks'][] = $callback;
    $callback();
}

function store_via_defined_vars_then_invoke(\Closure $callback): void
{
    $variables = get_defined_vars();
    $GLOBALS['callbacks'][] = $variables['callback'];
    $callback();
}

function store_via_compact_then_invoke(\Closure $callback): void
{
    $variables = compact('callback');
    $GLOBALS['callbacks'][] = $variables['callback'];
    $callback();
}

function store_via_aliased_compact_then_invoke(\Closure $callback): void
{
    $variables = snapshot('callback');
    $GLOBALS['callbacks'][] = $variables['callback'];
    $callback();
}

function store_via_variable_variable_then_invoke(\Closure $callback): void
{
    $name = 'callback';
    $GLOBALS['callbacks'][] = $$name;
    $callback();
}
";

const CALLER: &str = r"<?php

namespace App;

final class DeferredFailure extends \Exception {}

function exercise(): void
{
    invoke_now(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });

    invoke_after_compacting_other_variable(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });

    invoke_after_compacting_known_other_variable(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });

    invoke_after_other_variable_variable(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });

    invoke_after_isset_other_variable(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });

    invoke_after_mutating_variable_name(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });

    yield_then_invoke(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });

    store_then_invoke(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });

    store_via_defined_vars_then_invoke(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });

    store_via_compact_then_invoke(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });

    store_via_aliased_compact_then_invoke(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });

    store_via_variable_variable_then_invoke(/** @throws DeferredFailure */
        static function (): void {
            throw new DeferredFailure();
        });
}

exercise();
";

#[test]
fn uses_callback_invocation_metadata_across_files_and_rejects_escaping_callbacks() {
    let helper_file = std::sync::Arc::new(File::new(
        Cow::Borrowed(b"src/callbacks.php"),
        FileType::Host,
        None,
        Cow::Borrowed(HELPERS.as_bytes()),
    ));
    let caller_file = std::sync::Arc::new(File::new(
        Cow::Borrowed(b"src/caller.php"),
        FileType::Host,
        None,
        Cow::Borrowed(CALLER.as_bytes()),
    ));

    let arena = LocalArena::new();
    let helper_program = parse_file(&arena, &helper_file);
    let caller_program = parse_file(&arena, &caller_file);
    assert!(!helper_program.has_errors(), "the helper fixture should parse");
    assert!(!caller_program.has_errors(), "the caller fixture should parse");

    let helper_names = NameResolver::new(&arena).resolve(helper_program);
    let caller_names = NameResolver::new(&arena).resolve(caller_program);
    let mut codebase = PRELUDE.metadata.clone();
    codebase.extend(scan_program(&arena, &helper_file, helper_program, &helper_names, PHPVersion::PHP85));
    codebase.extend(scan_program(&arena, &caller_file, caller_program, &caller_names, PHPVersion::PHP85));

    let mut symbol_references = PRELUDE.symbol_references.clone();
    populate_codebase(&mut codebase, &mut symbol_references, WordSet::default(), foldhash::HashSet::default());

    let mut settings = Settings::new(PHPVersion::PHP85);
    settings.check_throws = true;
    settings.forbid_checked_exceptions_in_callables = true;
    settings.find_unused_definitions = false;
    settings.find_unused_expressions = false;
    settings.find_unused_parameters = false;

    let plugin_registry = PluginRegistry::with_library_providers();
    let analyzer = Analyzer::new(&arena, &caller_file, &caller_names, &codebase, &plugin_registry, settings);
    let mut result = AnalysisResult::new(symbol_references);
    analyzer.analyze(caller_program, &mut result).expect("analysis should succeed");

    let deferred_issues = result
        .issues
        .iter()
        .filter(|issue| issue.code.as_deref() == Some("unhandled-thrown-type"))
        .map(|issue| issue.message.to_string())
        .collect::<Vec<_>>();

    assert_eq!(deferred_issues.len(), 7, "unexpected deferred callback findings: {deferred_issues:#?}");
    for helper in [
        "store_then_invoke",
        "store_via_defined_vars_then_invoke",
        "store_via_compact_then_invoke",
        "store_via_aliased_compact_then_invoke",
        "store_via_variable_variable_then_invoke",
        "invoke_after_mutating_variable_name",
        "yield_then_invoke",
    ] {
        assert!(
            deferred_issues.iter().any(|issue| issue.contains(helper)),
            "missing deferred callback finding for {helper}: {deferred_issues:#?}"
        );
    }

    for helper in [
        "invoke_after_compacting_other_variable",
        "invoke_after_compacting_known_other_variable",
        "invoke_after_other_variable_variable",
        "invoke_after_isset_other_variable",
    ] {
        assert!(
            deferred_issues.iter().all(|issue| !issue.contains(helper)),
            "an unrelated compact or variable-variable use should not escape the callback: {deferred_issues:#?}"
        );
    }
}
