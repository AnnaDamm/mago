use foldhash::HashSet;
use mago_codex::identifier::function_like::FunctionLikeIdentifier;
use mago_span::HasSpan;
use mago_span::Span;
use mago_syntax::cst::ArrowFunction;
use mago_syntax::cst::Closure;
use mago_syntax::cst::Expression;
use mago_syntax::cst::FunctionCall;
use mago_syntax::walker::Walker;

use crate::artifacts::AnalysisArtifacts;
use crate::artifacts::CallableInvocationKind;
use crate::context::block::BlockContext;
use crate::invocation::InvocationTarget;
use crate::utils::misc::unwrap_expression;

#[must_use]
pub(crate) fn classify_callback(
    target: &InvocationTarget<'_>,
    parameter_offset: usize,
    argument_offset: usize,
    argument_count: usize,
    is_unpacked: bool,
    artifacts: &AnalysisArtifacts,
) -> Option<CallableInvocationKind> {
    let identifier = target.get_function_like_identifier().copied()?;
    let metadata = target.get_function_like_metadata();

    if metadata.is_some_and(|metadata| metadata.flags.has_yield()) {
        return None;
    }

    let is_builtin = metadata.is_some_and(|metadata| metadata.flags.is_built_in());

    if is_builtin
        && is_builtin_callback_parameter(&identifier, parameter_offset, argument_offset, argument_count, is_unpacked)
    {
        return Some(CallableInvocationKind::Immediately);
    }

    if target
        .get_function_like_metadata()
        .is_some_and(|metadata| metadata.synchronously_invoked_callable_parameters.contains(&parameter_offset))
    {
        return Some(CallableInvocationKind::Immediately);
    }

    artifacts
        .has_callback_parameter_invocation(&identifier, parameter_offset)
        .then_some(CallableInvocationKind::Immediately)
}

fn is_builtin_callback_parameter(
    identifier: &FunctionLikeIdentifier,
    parameter_offset: usize,
    argument_offset: usize,
    argument_count: usize,
    is_unpacked: bool,
) -> bool {
    let FunctionLikeIdentifier::Function(name) = identifier else {
        return false;
    };
    let name = name.as_bytes().rsplit(|byte| *byte == b'\\').next().unwrap_or(name.as_bytes());

    let fixed_parameter = if is_builtin_name(
        name,
        &[
            &b"array_any"[..],
            b"array_all",
            b"array_find",
            b"array_find_key",
            b"array_filter",
            b"array_reduce",
            b"array_walk",
            b"array_walk_recursive",
            b"usort",
            b"uasort",
            b"uksort",
        ],
    ) {
        1
    } else if is_builtin_name(name, &[b"array_map"]) {
        0
    } else if is_builtin_name(name, &[b"preg_replace_callback"]) {
        1
    } else if is_builtin_name(
        name,
        &[
            b"preg_replace_callback_array",
            b"call_user_func",
            b"call_user_func_array",
            b"forward_static_call",
            b"forward_static_call_array",
        ],
    ) {
        0
    } else if is_builtin_name(name, &[b"iterator_apply"]) {
        1
    } else {
        return is_builtin_variadic_callback_parameter(
            name,
            parameter_offset,
            argument_offset,
            argument_count,
            is_unpacked,
        );
    };

    parameter_offset == fixed_parameter
}

fn is_builtin_name(name: &[u8], candidates: &[&[u8]]) -> bool {
    candidates.iter().any(|candidate| name.eq_ignore_ascii_case(candidate))
}

fn is_builtin_variadic_callback_parameter(
    name: &[u8],
    parameter_offset: usize,
    argument_offset: usize,
    argument_count: usize,
    is_unpacked: bool,
) -> bool {
    let callback_count = if [
        &b"array_diff_ukey"[..],
        b"array_udiff",
        b"array_udiff_assoc",
        b"array_diff_uassoc",
        b"array_uintersect_assoc",
        b"array_intersect_uassoc",
        b"array_intersect_ukey",
        b"array_uintersect",
    ]
    .iter()
    .any(|candidate| name.eq_ignore_ascii_case(candidate))
    {
        1
    } else if [b"array_udiff_uassoc".as_slice(), b"array_uintersect_uassoc"]
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
    {
        2
    } else {
        return false;
    };

    parameter_offset == 1
        && !is_unpacked
        && argument_count >= callback_count
        && argument_offset >= argument_count - callback_count
}

#[must_use]
pub(crate) fn immediate_callable_parameter_offset(
    block_context: &BlockContext<'_>,
    expression: &Expression<'_>,
) -> Option<usize> {
    let Expression::Variable(variable) = unwrap_expression(expression) else {
        return None;
    };
    let variable_name = crate::utils::expression::get_variable_id(variable)?;
    let variable_name = variable_name.strip_prefix(b"$").unwrap_or(variable_name);
    let function_like = block_context.scope.get_function_like()?;

    function_like.parameters.iter().position(|parameter| {
        parameter.get_type_metadata().is_some_and(|type_metadata| type_metadata.type_union.has_callable())
            && parameter.get_name().0.as_bytes().strip_prefix(b"$").unwrap_or(parameter.get_name().0.as_bytes())
                == variable_name
    })
}

#[must_use]
pub(crate) fn callable_parameter_offsets_referenced_in_expression(
    block_context: &BlockContext<'_>,
    expression: &Expression<'_>,
) -> Vec<usize> {
    let Some(function_like) = block_context.scope.get_function_like() else {
        return Vec::new();
    };

    let mut variables = crate::utils::expression::variable::get_variables_referenced_in_expression(expression, true);
    let mut invocation_target_spans = HashSet::default();
    CallableInvocationTargetCollector.walk_expression(expression, &mut invocation_target_spans);

    // Invoking a callback parameter reads the callable but does not let it escape.
    // Remove only direct callee occurrences so that a separate use of the same
    // parameter elsewhere in the expression is still recognized. Nested callable
    // bodies are skipped because references captured by them do escape this scope.
    variables.retain(|(_, span)| !invocation_target_spans.contains(span));

    function_like
        .parameters
        .iter()
        .enumerate()
        .filter_map(|(offset, parameter)| {
            if !parameter.get_type_metadata().is_some_and(|metadata| metadata.type_union.has_callable()) {
                return None;
            }

            let parameter_name =
                parameter.get_name().0.as_bytes().strip_prefix(b"$").unwrap_or(parameter.get_name().0.as_bytes());
            variables
                .iter()
                .any(|(name, _)| name.strip_prefix(b"$").unwrap_or(name) == parameter_name)
                .then_some(offset)
        })
        .collect()
}

#[derive(Debug, Default)]
struct CallableInvocationTargetCollector;

impl<'ast, 'arena> Walker<'ast, 'arena, HashSet<Span>> for CallableInvocationTargetCollector {
    fn walk_function_call(&self, function_call: &'ast FunctionCall<'arena>, spans: &mut HashSet<Span>) {
        if let Expression::Variable(variable) = unwrap_expression(function_call.function)
            && crate::utils::expression::get_variable_id(variable).is_some()
        {
            spans.insert(variable.span());
        }

        mago_syntax::walker::walk_function_call(self, function_call, spans);
    }

    fn walk_arrow_function(&self, _: &'ast ArrowFunction<'arena>, _: &mut HashSet<Span>) {}

    fn walk_closure(&self, _: &'ast Closure<'arena>, _: &mut HashSet<Span>) {}
}
