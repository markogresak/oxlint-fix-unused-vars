use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::AstKind;
use oxc_semantic::Semantic;
use oxc_span::{GetSpan, Span};

use crate::{ArgsOption, NoUnusedVarsOptions, Removal, UnusedBinding, UnusedKind};

pub(crate) fn remove_unused(
    source: &str,
    semantic: &Semantic<'_>,
    unused: &[UnusedBinding],
    options: &NoUnusedVarsOptions,
) -> (String, Vec<Removal>) {
    let unused_parameter_spans = unused
        .iter()
        .filter(|binding| binding.kind == UnusedKind::Parameter)
        .filter_map(|binding| {
            let declaration = semantic.symbol_declaration(binding.symbol_id);
            std::iter::once(declaration)
                .chain(semantic.nodes().ancestors(declaration.id()))
                .find_map(|node| match node.kind() {
                    AstKind::FormalParameter(parameter)
                        if matches!(
                            parameter.pattern,
                            oxc_ast::ast::BindingPattern::BindingIdentifier(_)
                        ) && parameter.pattern.span() == binding.span
                            && !formal_parameter_may_have_side_effects(parameter) =>
                    {
                        Some(parameter.span)
                    }
                    AstKind::FormalParameterRest(parameter)
                        if matches!(
                            parameter.rest.argument,
                            oxc_ast::ast::BindingPattern::BindingIdentifier(_)
                        ) && parameter.rest.argument.span() == binding.span =>
                    {
                        Some(parameter.span)
                    }
                    _ => None,
                })
        })
        .collect::<Vec<_>>();
    let mut edits = Vec::new();
    let mut removals = Vec::new();
    let unused_variable_spans = unused
        .iter()
        .filter(|binding| binding.kind == UnusedKind::Variable)
        .map(|binding| (binding.span.start, binding.span.end))
        .collect::<BTreeSet<_>>();
    let mut fully_unused_groups = BTreeMap::<(u32, u32), FullyUnusedVariableGroup>::new();
    let mut variable_groups = BTreeMap::<(u32, u32), VariableGroup>::new();

    for binding in unused
        .iter()
        .filter(|binding| binding.kind != UnusedKind::Import)
    {
        if semantic.scoping().symbol_declarations(binding.symbol_id).nth(1).is_some() {
            continue;
        }
        if matches!(
            binding.kind,
            UnusedKind::Variable
                | UnusedKind::Class
                | UnusedKind::Function
                | UnusedKind::Parameter
                | UnusedKind::CatchParameter
        ) && semantic
            .scoping()
            .get_resolved_references(binding.symbol_id)
            .any(|reference| reference.is_write())
        {
            continue;
        }
        if binding.kind == UnusedKind::Variable {
            if let Some(info) =
                fully_unused_variable_info(semantic, binding, &unused_variable_spans)
            {
                fully_unused_groups
                    .entry((info.declaration_span.start, info.declaration_span.end))
                    .or_insert_with(|| FullyUnusedVariableGroup {
                        declaration_span: info.declaration_span,
                        whole_span: info.whole_span,
                        whole_removal_is_safe: info.whole_removal_is_safe,
                        is_loop_header: info.is_loop_header,
                        has_destructuring: info.has_destructuring,
                        has_side_effects: info.has_side_effects,
                        unused: Vec::new(),
                    })
                    .unused
                    .push(binding);
                continue;
            }
            if let Some(info) = simple_variable_info(semantic, binding) {
                if info.has_side_effects {
                    continue;
                }
                let group = variable_groups
                    .entry((info.declaration_span.start, info.declaration_span.end))
                    .or_insert_with(|| VariableGroup {
                        declarators: info.declarators,
                        whole_span: info.whole_span,
                        whole_removal_is_safe: info.whole_removal_is_safe,
                        is_loop_header: info.is_loop_header,
                        unused: Vec::new(),
                    });
                group.unused.push((binding, info.index));
                continue;
            }
        }

        if let Some(removal) =
            removal_for(source, semantic, binding, options, &unused_parameter_spans)
        {
            edits.push(Span::new(removal.start, removal.end));
            removals.push(removal);
        }
    }

    for group in fully_unused_groups.into_values() {
        let contains_destructuring_comment = group.has_destructuring
            && contains_comment(
                &source[group.declaration_span.start as usize..group.declaration_span.end as usize],
            );
        if !group.is_loop_header
            && group.whole_removal_is_safe
            && !contains_destructuring_comment
            && !group.has_side_effects
        {
            let span = with_declaration_context(source, group.whole_span);
            edits.push(span);
            removals.extend(
                group
                    .unused
                    .into_iter()
                    .map(|binding| removal(binding, span)),
            );
        }
    }
    for group in variable_groups.into_values() {
        add_variable_group_removals(source, &group, &mut edits, &mut removals);
    }

    removals.sort_by_key(|removal| (removal.start, std::cmp::Reverse(removal.end)));

    edits.sort_by_key(|span| (span.start, std::cmp::Reverse(span.end)));
    let mut merged: Vec<Span> = Vec::new();
    for removal in edits {
        if let Some(previous) = merged.last_mut() {
            if removal.start <= previous.end {
                if removal.end > previous.end {
                    previous.end = removal.end;
                }
                continue;
            }
        }
        merged.push(removal);
    }

    let mut updated = source.to_owned();
    for removal in merged.iter().rev() {
        updated.replace_range(removal.start as usize..removal.end as usize, "");
    }
    (updated, removals)
}

struct SimpleVariableInfo {
    declaration_span: Span,
    declarators: Vec<Span>,
    whole_span: Span,
    whole_removal_is_safe: bool,
    is_loop_header: bool,
    has_side_effects: bool,
    index: usize,
}

struct VariableGroup<'a> {
    declarators: Vec<Span>,
    whole_span: Span,
    whole_removal_is_safe: bool,
    is_loop_header: bool,
    unused: Vec<(&'a UnusedBinding, usize)>,
}

struct FullyUnusedVariableInfo {
    declaration_span: Span,
    whole_span: Span,
    whole_removal_is_safe: bool,
    is_loop_header: bool,
    has_destructuring: bool,
    has_side_effects: bool,
}

struct FullyUnusedVariableGroup<'a> {
    declaration_span: Span,
    whole_span: Span,
    whole_removal_is_safe: bool,
    is_loop_header: bool,
    has_destructuring: bool,
    has_side_effects: bool,
    unused: Vec<&'a UnusedBinding>,
}

fn fully_unused_variable_info(
    semantic: &Semantic<'_>,
    binding: &UnusedBinding,
    unused_spans: &BTreeSet<(u32, u32)>,
) -> Option<FullyUnusedVariableInfo> {
    let declaration = semantic.symbol_declaration(binding.symbol_id);
    let nodes = std::iter::once(declaration)
        .chain(semantic.nodes().ancestors(declaration.id()))
        .collect::<Vec<_>>();
    let (declaration_index, variable_declaration) =
        nodes
            .iter()
            .enumerate()
            .find_map(|(index, node)| match node.kind() {
                AstKind::VariableDeclaration(declaration) => Some((index, declaration)),
                _ => None,
            })?;
    let fully_unused = variable_declaration.declarations.iter().all(|declarator| {
        let mut spans = Vec::new();
        binding_identifier_spans(&declarator.id, &mut spans);
        !spans.is_empty()
            && spans
                .iter()
                .all(|span| unused_spans.contains(&(span.start, span.end)))
    });
    if !fully_unused {
        return None;
    }

    let (whole_span, whole_removal_is_safe) =
        whole_statement_context(&nodes, declaration_index, variable_declaration.span);
    let is_loop_header = nodes.get(declaration_index + 1).is_some_and(|node| {
        matches!(
            node.kind(),
            AstKind::ForStatement(_) | AstKind::ForInStatement(_) | AstKind::ForOfStatement(_)
        )
    });
    let has_destructuring = variable_declaration.declarations.iter().any(|declarator| {
        !matches!(
            declarator.id,
            oxc_ast::ast::BindingPattern::BindingIdentifier(_)
        )
    });
    Some(FullyUnusedVariableInfo {
        declaration_span: variable_declaration.span,
        whole_span,
        whole_removal_is_safe,
        is_loop_header,
        has_destructuring,
        has_side_effects: variable_declaration_may_have_side_effects(variable_declaration),
    })
}

fn binding_identifier_spans(pattern: &oxc_ast::ast::BindingPattern<'_>, spans: &mut Vec<Span>) {
    use oxc_ast::ast::BindingPattern;

    match pattern {
        BindingPattern::BindingIdentifier(identifier) => spans.push(identifier.span),
        BindingPattern::AssignmentPattern(assignment) => {
            binding_identifier_spans(&assignment.left, spans);
        }
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                binding_identifier_spans(&property.value, spans);
            }
            if let Some(rest) = &object.rest {
                binding_identifier_spans(&rest.argument, spans);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                binding_identifier_spans(element, spans);
            }
            if let Some(rest) = &array.rest {
                binding_identifier_spans(&rest.argument, spans);
            }
        }
    }
}

fn simple_variable_info(
    semantic: &Semantic<'_>,
    binding: &UnusedBinding,
) -> Option<SimpleVariableInfo> {
    let declaration = semantic.symbol_declaration(binding.symbol_id);
    let nodes = std::iter::once(declaration)
        .chain(semantic.nodes().ancestors(declaration.id()))
        .collect::<Vec<_>>();
    let declarator = nodes.iter().find_map(|node| match node.kind() {
        AstKind::VariableDeclarator(declarator) => Some(declarator),
        _ => None,
    })?;
    if declarator.id.span() != binding.span {
        return None;
    }
    let (declaration_index, variable_declaration) =
        nodes
            .iter()
            .enumerate()
            .find_map(|(index, node)| match node.kind() {
                AstKind::VariableDeclaration(declaration) => Some((index, declaration)),
                _ => None,
            })?;
    let index = variable_declaration
        .declarations
        .iter()
        .position(|candidate| candidate.span == declarator.span)?;
    let (whole_span, whole_removal_is_safe) =
        whole_statement_context(&nodes, declaration_index, variable_declaration.span);
    let is_loop_header = nodes.get(declaration_index + 1).is_some_and(|node| {
        matches!(
            node.kind(),
            AstKind::ForStatement(_) | AstKind::ForInStatement(_) | AstKind::ForOfStatement(_)
        )
    });

    Some(SimpleVariableInfo {
        declaration_span: variable_declaration.span,
        declarators: variable_declaration
            .declarations
            .iter()
            .map(|declarator| declarator.span)
            .collect(),
        whole_span,
        whole_removal_is_safe,
        is_loop_header,
        has_side_effects: declarator_may_have_side_effects(declarator),
        index,
    })
}

fn add_variable_group_removals(
    source: &str,
    group: &VariableGroup<'_>,
    edits: &mut Vec<Span>,
    removals: &mut Vec<Removal>,
) {
    if group.is_loop_header {
        return;
    }

    let mut unused = group
        .unused
        .iter()
        .map(|(_, index)| *index)
        .collect::<Vec<_>>();
    unused.sort_unstable();
    unused.dedup();
    if unused.len() == group.declarators.len() {
        if !group.whole_removal_is_safe {
            return;
        }
        let span = with_declaration_context(source, group.whole_span);
        edits.push(span);
        for (binding, _) in &group.unused {
            removals.push(removal(binding, span));
        }
        return;
    }

    let mut spans_by_index = BTreeMap::new();
    let mut run_start = 0;
    while run_start < unused.len() {
        let mut run_end = run_start;
        while run_end + 1 < unused.len() && unused[run_end + 1] == unused[run_end] + 1 {
            run_end += 1;
        }
        let first = unused[run_start];
        let last = unused[run_end];
        let span = if first == 0 {
            Span::new(
                group.declarators[first].start,
                group.declarators[last + 1].start,
            )
        } else {
            Span::new(
                group.declarators[first - 1].end,
                group.declarators[last].end,
            )
        };
        if contains_comment(&source[span.start as usize..span.end as usize]) {
            run_start = run_end + 1;
            continue;
        }
        edits.push(span);
        for index in &unused[run_start..=run_end] {
            spans_by_index.insert(*index, span);
        }
        run_start = run_end + 1;
    }
    for (binding, index) in &group.unused {
        if let Some(span) = spans_by_index.get(index) {
            removals.push(removal(binding, *span));
        }
    }
}

fn removal_for(
    source: &str,
    semantic: &Semantic<'_>,
    binding: &UnusedBinding,
    options: &NoUnusedVarsOptions,
    unused_parameter_spans: &[Span],
) -> Option<Removal> {
    if semantic.scoping().symbol_declarations(binding.symbol_id).nth(1).is_some() {
        return None;
    }
    let declaration = semantic.symbol_declaration(binding.symbol_id);
    let nodes = semantic.nodes();
    let all_nodes = std::iter::once(declaration).chain(nodes.ancestors(declaration.id()));

    let span = match binding.kind {
        UnusedKind::Variable => {
            let ancestors = all_nodes.collect::<Vec<_>>();
            if variable_declaration_is_loop_header(&ancestors) {
                return None;
            }
            // Partial destructuring edits can skip getters/defaults or change rest/iterator
            // consumption; leave them alone in v0.1.
            let _ = ancestors;
            return None;
        }
        UnusedKind::Parameter
            if matches!(options.args, ArgsOption::AfterUsed | ArgsOption::All) =>
        {
            parameter_span(source, all_nodes, binding.span, unused_parameter_spans)?
        }
        UnusedKind::Parameter => return None,
        UnusedKind::CatchParameter => catch_parameter_span(source, all_nodes)?,
        UnusedKind::Type
            if matches!(
                declaration.kind(),
                AstKind::TSTypeAliasDeclaration(_) | AstKind::TSInterfaceDeclaration(_)
            ) =>
        {
            whole_declaration_span(source, all_nodes)?
        }
        UnusedKind::Class => {
            let ancestors = all_nodes.collect::<Vec<_>>();
            if ancestors.iter().any(|node| match node.kind() {
                AstKind::Class(class) => class_may_have_side_effects(class),
                _ => false,
            }) {
                return None;
            }
            whole_declaration_span(source, ancestors.into_iter())?
        }
        UnusedKind::Enum => {
            let ancestors = all_nodes.collect::<Vec<_>>();
            if ancestors.iter().any(|node| match node.kind() {
                AstKind::TSEnumDeclaration(declaration) => {
                    enum_may_have_side_effects(declaration)
                }
                _ => false,
            }) {
                return None;
            }
            whole_declaration_span(source, ancestors.into_iter())?
        }
        UnusedKind::Function => whole_declaration_span(source, all_nodes)?,
        UnusedKind::Type => return None,
        UnusedKind::Import | UnusedKind::Other => return None,
    };

    Some(removal(binding, span))
}

fn variable_declaration_is_loop_header(nodes: &[&oxc_semantic::AstNode<'_>]) -> bool {
    nodes
        .iter()
        .position(|node| matches!(node.kind(), AstKind::VariableDeclaration(_)))
        .and_then(|index| nodes.get(index + 1))
        .is_some_and(|node| {
            matches!(
                node.kind(),
                AstKind::ForStatement(_) | AstKind::ForInStatement(_) | AstKind::ForOfStatement(_)
            )
        })
}

fn parameter_span<'a>(
    source: &str,
    nodes: impl Iterator<Item = &'a oxc_semantic::AstNode<'a>>,
    binding_span: Span,
    unused_parameter_spans: &[Span],
) -> Option<Span> {
    let nodes = nodes.collect::<Vec<_>>();
    let parameter = nodes.iter().find_map(|node| match node.kind() {
        AstKind::FormalParameter(parameter)
            if matches!(
                parameter.pattern,
                oxc_ast::ast::BindingPattern::BindingIdentifier(_)
            ) && parameter.pattern.span() == binding_span
                && !formal_parameter_may_have_side_effects(parameter) =>
        {
            Some(parameter.span)
        }
        AstKind::FormalParameterRest(parameter)
            if matches!(
                parameter.rest.argument,
                oxc_ast::ast::BindingPattern::BindingIdentifier(_)
            ) && parameter.rest.argument.span() == binding_span =>
        {
            Some(parameter.span)
        }
        _ => None,
    })?;
    let parameters = nodes.iter().find_map(|node| match node.kind() {
        AstKind::FormalParameters(parameters) => Some(parameters),
        _ => None,
    })?;
    if parameters.items.iter().any(|item| {
        unused_parameter_spans.contains(&item.span) && formal_parameter_may_have_side_effects(item)
    }) {
        return None;
    }
    let mut ordered = parameters
        .items
        .iter()
        .map(|item| item.span)
        .collect::<Vec<_>>();
    if let Some(rest) = &parameters.rest {
        ordered.push(rest.span);
    }
    let index = ordered.iter().position(|span| *span == parameter)?;
    if !ordered[index..]
        .iter()
        .all(|span| unused_parameter_spans.contains(span))
        || index > 0 && unused_parameter_spans.contains(&ordered[index - 1])
    {
        return None;
    }
    if ordered.len() == 1
        && nodes
            .iter()
            .any(|node| matches!(node.kind(), AstKind::ArrowFunctionExpression(_)))
        && parameter
            .start
            .checked_sub(1)
            .and_then(|index| source.as_bytes().get(index as usize))
            != Some(&b'(')
    {
        return None;
    }

    let bytes = source.as_bytes();
    let mut start = parameter.start as usize;
    while start > 0 && bytes[start - 1].is_ascii_whitespace() {
        start -= 1;
    }
    if start > 0 && bytes[start - 1] == b',' {
        start -= 1;
    }
    Some(Span::new(start as u32, ordered.last()?.end))
}

fn catch_parameter_span<'a>(
    source: &str,
    mut nodes: impl Iterator<Item = &'a oxc_semantic::AstNode<'a>>,
) -> Option<Span> {
    let parameter = nodes.find_map(|node| match node.kind() {
        AstKind::CatchParameter(parameter)
            if matches!(
                parameter.pattern,
                oxc_ast::ast::BindingPattern::BindingIdentifier(_)
            ) =>
        {
            Some(parameter.span)
        }
        _ => None,
    })?;
    let bytes = source.as_bytes();
    let mut start = parameter.start as usize;
    while start > 0 {
        let previous = bytes[start - 1];
        if previous == b'(' {
            start -= 1;
            break;
        }
        if previous.is_ascii_whitespace() {
            start -= 1;
            continue;
        }
        // Comment or unexpected token between `(` and the binding — skip.
        return None;
    }
    let mut end = parameter.end as usize;
    while end < bytes.len() && bytes[end].is_ascii_whitespace() {
        end += 1;
    }
    if bytes.get(end) == Some(&b')') {
        end += 1;
        while end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
            end += 1;
        }
    } else {
        return None;
    }
    if contains_comment(&source[start..end]) {
        return None;
    }
    Some(Span::new(start as u32, end as u32))
}

fn whole_declaration_span<'a>(
    source: &str,
    nodes: impl Iterator<Item = &'a oxc_semantic::AstNode<'a>>,
) -> Option<Span> {
    let nodes = nodes.collect::<Vec<_>>();
    let (declaration_index, declaration_span) =
        nodes
            .iter()
            .enumerate()
            .find_map(|(index, node)| match node.kind() {
                AstKind::Function(function) => Some((index, function.span)),
                AstKind::Class(class) => Some((index, class.span)),
                AstKind::TSTypeAliasDeclaration(declaration) => Some((index, declaration.span)),
                AstKind::TSInterfaceDeclaration(declaration) => Some((index, declaration.span)),
                AstKind::TSEnumDeclaration(declaration) => Some((index, declaration.span)),
                _ => None,
            })?;
    let (span, safe) = whole_statement_context(&nodes, declaration_index, declaration_span);
    safe.then(|| with_declaration_context(source, span))
}

fn whole_statement_context(
    nodes: &[&oxc_semantic::AstNode<'_>],
    declaration_index: usize,
    declaration_span: Span,
) -> (Span, bool) {
    let mut parent_index = declaration_index + 1;
    let span = match nodes.get(parent_index).map(|node| node.kind()) {
        Some(AstKind::ExportNamedDeclaration(declaration)) => {
            parent_index += 1;
            declaration.span
        }
        Some(AstKind::ExportDefaultDeclaration(declaration)) => {
            parent_index += 1;
            declaration.span
        }
        _ => declaration_span,
    };
    let safe = nodes.get(parent_index).is_some_and(|node| {
        matches!(
            node.kind(),
            AstKind::Program(_)
                | AstKind::BlockStatement(_)
                | AstKind::FunctionBody(_)
                | AstKind::SwitchCase(_)
                | AstKind::TSModuleBlock(_)
        )
    });
    (span, safe)
}


#[derive(Clone, Copy)]
enum PurityContext {
    /// Direct value position: reading a local/identifier is treated as pure.
    Value,
    /// ToString/ToPrimitive coercion may invoke user code.
    Coerced,
}

fn expression_may_have_side_effects(expression: &oxc_ast::ast::Expression<'_>) -> bool {
    !expression_is_definitely_pure(expression, PurityContext::Value)
}

fn expression_is_definitely_pure(
    expression: &oxc_ast::ast::Expression<'_>,
    context: PurityContext,
) -> bool {
    use oxc_ast::ast::Expression;
    use oxc_syntax::operator::{BinaryOperator, UnaryOperator};

    match expression {
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::Super(_)
        | Expression::ThisExpression(_)
        | Expression::ArrowFunctionExpression(_)
        | Expression::FunctionExpression(_) => true,
        // Global getters / TDZ / unresolved references can observe removal.
        Expression::Identifier(_) => false,
        Expression::ClassExpression(class) => !class_may_have_side_effects(class),
        Expression::TemplateLiteral(template) => template
            .expressions
            .iter()
            .all(|expression| expression_is_definitely_pure(expression, PurityContext::Coerced)),
        Expression::UnaryExpression(unary)
            if !matches!(unary.operator, UnaryOperator::Delete | UnaryOperator::Void) =>
        {
            let child_context = if matches!(
                unary.operator,
                UnaryOperator::UnaryPlus
                    | UnaryOperator::UnaryNegation
                    | UnaryOperator::LogicalNot
                    | UnaryOperator::BitwiseNot
            ) {
                PurityContext::Coerced
            } else {
                context
            };
            expression_is_definitely_pure(&unary.argument, child_context)
        }
        Expression::SequenceExpression(sequence) => sequence
            .expressions
            .iter()
            .all(|expression| expression_is_definitely_pure(expression, context)),
        Expression::ConditionalExpression(conditional) => {
            expression_is_definitely_pure(&conditional.test, PurityContext::Coerced)
                && expression_is_definitely_pure(&conditional.consequent, context)
                && expression_is_definitely_pure(&conditional.alternate, context)
        }
        Expression::LogicalExpression(logical) => {
            expression_is_definitely_pure(&logical.left, PurityContext::Coerced)
                && expression_is_definitely_pure(&logical.right, context)
        }
        Expression::BinaryExpression(binary) => {
            let coerced = matches!(
                binary.operator,
                BinaryOperator::Addition
                    | BinaryOperator::Subtraction
                    | BinaryOperator::Multiplication
                    | BinaryOperator::Division
                    | BinaryOperator::Remainder
                    | BinaryOperator::ShiftLeft
                    | BinaryOperator::ShiftRight
                    | BinaryOperator::ShiftRightZeroFill
                    | BinaryOperator::BitwiseOR
                    | BinaryOperator::BitwiseXOR
                    | BinaryOperator::BitwiseAnd
                    | BinaryOperator::Exponential
                    | BinaryOperator::Equality
                    | BinaryOperator::Inequality
                    | BinaryOperator::StrictEquality
                    | BinaryOperator::StrictInequality
                    | BinaryOperator::LessThan
                    | BinaryOperator::LessEqualThan
                    | BinaryOperator::GreaterThan
                    | BinaryOperator::GreaterEqualThan
                    | BinaryOperator::In
                    | BinaryOperator::Instanceof
            );
            let child = if coerced {
                PurityContext::Coerced
            } else {
                context
            };
            expression_is_definitely_pure(&binary.left, child)
                && expression_is_definitely_pure(&binary.right, child)
        }
        Expression::ParenthesizedExpression(parenthesized) => {
            expression_is_definitely_pure(&parenthesized.expression, context)
        }
        Expression::TSAsExpression(expression) => {
            expression_is_definitely_pure(&expression.expression, context)
        }
        Expression::TSSatisfiesExpression(expression) => {
            expression_is_definitely_pure(&expression.expression, context)
        }
        Expression::TSTypeAssertion(expression) => {
            expression_is_definitely_pure(&expression.expression, context)
        }
        Expression::TSNonNullExpression(expression) => {
            expression_is_definitely_pure(&expression.expression, context)
        }
        Expression::TSInstantiationExpression(expression) => {
            expression_is_definitely_pure(&expression.expression, context)
        }
        Expression::ArrayExpression(array) => array.elements.iter().all(|element| match element {
            oxc_ast::ast::ArrayExpressionElement::Elision(_) => true,
            oxc_ast::ast::ArrayExpressionElement::SpreadElement(_) => false,
            other => other
                .as_expression()
                .is_some_and(|expression| expression_is_definitely_pure(expression, context)),
        }),
        Expression::ObjectExpression(object) => {
            object.properties.iter().all(|property| match property {
                oxc_ast::ast::ObjectPropertyKind::SpreadProperty(_) => false,
                oxc_ast::ast::ObjectPropertyKind::ObjectProperty(property) => {
                    !property.method
                        && matches!(property.kind, oxc_ast::ast::PropertyKind::Init)
                        && property.key.as_expression().is_none_or(|expression| {
                            expression_is_definitely_pure(expression, PurityContext::Coerced)
                        })
                        && expression_is_definitely_pure(&property.value, context)
                }
            })
        }
        _ => false,
    }
}

fn binding_pattern_may_have_side_effects(pattern: &oxc_ast::ast::BindingPattern<'_>) -> bool {
    use oxc_ast::ast::BindingPattern;

    match pattern {
        BindingPattern::BindingIdentifier(_) => false,
        BindingPattern::AssignmentPattern(assignment) => {
            expression_may_have_side_effects(&assignment.right)
                || binding_pattern_may_have_side_effects(&assignment.left)
        }
        BindingPattern::ObjectPattern(object) => {
            object.properties.iter().any(|property| {
                binding_pattern_may_have_side_effects(&property.value)
            }) || object
                .rest
                .as_ref()
                .is_some_and(|rest| binding_pattern_may_have_side_effects(&rest.argument))
        }
        BindingPattern::ArrayPattern(array) => {
            array.elements.iter().flatten().any(binding_pattern_may_have_side_effects)
                || array
                    .rest
                    .as_ref()
                    .is_some_and(|rest| binding_pattern_may_have_side_effects(&rest.argument))
        }
    }
}

fn formal_parameter_may_have_side_effects(
    parameter: &oxc_ast::ast::FormalParameter<'_>,
) -> bool {
    binding_pattern_may_have_side_effects(&parameter.pattern)
        || parameter
            .initializer
            .as_ref()
            .is_some_and(|expression| expression_may_have_side_effects(expression))
}

fn declarator_may_have_side_effects(declarator: &oxc_ast::ast::VariableDeclarator<'_>) -> bool {
    matches!(
        declarator.kind,
        oxc_ast::ast::VariableDeclarationKind::Using
            | oxc_ast::ast::VariableDeclarationKind::AwaitUsing
    ) || binding_pattern_may_have_side_effects(&declarator.id)
        || declarator
            .init
            .as_ref()
            .is_some_and(expression_may_have_side_effects)
        // Destructuring can invoke iterators/getters on the initializer.
        || (!matches!(
            declarator.id,
            oxc_ast::ast::BindingPattern::BindingIdentifier(_)
        ) && declarator.init.is_some())
}

fn variable_declaration_may_have_side_effects(
    declaration: &oxc_ast::ast::VariableDeclaration<'_>,
) -> bool {
    declaration
        .declarations
        .iter()
        .any(declarator_may_have_side_effects)
}

fn class_may_have_side_effects(class: &oxc_ast::ast::Class<'_>) -> bool {
    !class.decorators.is_empty()
        || class.super_class.is_some()
        || class.body.body.iter().any(|element| match element {
            oxc_ast::ast::ClassElement::StaticBlock(_) => true,
            oxc_ast::ast::ClassElement::PropertyDefinition(property) => {
                !property.decorators.is_empty()
                    || property.computed
                    || property
                        .value
                        .as_ref()
                        .is_some_and(expression_may_have_side_effects)
                    || property
                        .key
                        .as_expression()
                        .is_some_and(expression_may_have_side_effects)
            }
            oxc_ast::ast::ClassElement::MethodDefinition(method) => {
                !method.decorators.is_empty()
                    || method.computed
                    || method
                        .key
                        .as_expression()
                        .is_some_and(expression_may_have_side_effects)
            }
            oxc_ast::ast::ClassElement::AccessorProperty(property) => {
                !property.decorators.is_empty()
                    || property.computed
                    || property
                        .value
                        .as_ref()
                        .is_some_and(expression_may_have_side_effects)
            }
            oxc_ast::ast::ClassElement::TSIndexSignature(_) => false,
        })
}

fn enum_may_have_side_effects(declaration: &oxc_ast::ast::TSEnumDeclaration<'_>) -> bool {
    declaration.body.members.iter().any(|member| {
        member
            .initializer
            .as_ref()
            .is_some_and(expression_may_have_side_effects)
    })
}

fn contains_comment(source: &str) -> bool {
    source.contains("//") || source.contains("/*")
}

fn removal(binding: &UnusedBinding, span: Span) -> Removal {
    Removal {
        name: binding.name.clone(),
        start: span.start,
        end: span.end,
        kind: kind_name(binding.kind).to_owned(),
    }
}

fn with_declaration_context(source: &str, span: Span) -> Span {
    let bytes = source.as_bytes();
    let start = leading_comment_start(source, span.start as usize);

    let mut end = span.end as usize;
    if bytes.get(end) == Some(&b';') {
        end += 1;
    }
    if bytes.get(end) == Some(&b'\r') {
        end += 1;
    }
    if bytes.get(end) == Some(&b'\n') {
        end += 1;
    }
    Span::new(start as u32, end as u32)
}

fn leading_comment_start(source: &str, declaration_start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut cursor = declaration_start;
    let declaration_line_start = source[..declaration_start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let mut start = if source[declaration_line_start..declaration_start]
        .trim()
        .is_empty()
    {
        declaration_line_start
    } else {
        declaration_start
    };
    loop {
        let mut content_end = cursor;
        while content_end > 0 && bytes[content_end - 1].is_ascii_whitespace() {
            content_end -= 1;
        }
        if source[content_end..cursor]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            > 1
        {
            break;
        }

        if source[..content_end].ends_with("*/") {
            let Some(opening) = source[..content_end - 2].rfind("/*") else {
                break;
            };
            let line_start = source[..opening].rfind('\n').map_or(0, |index| index + 1);
            if !source[line_start..opening].trim().is_empty() {
                break;
            }
            start = line_start;
            cursor = line_start;
            continue;
        }

        let line_start = source[..content_end]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        if source[line_start..content_end]
            .trim_start()
            .starts_with("//")
        {
            start = line_start;
            cursor = line_start;
            continue;
        }

        break;
    }
    start
}

fn kind_name(kind: UnusedKind) -> &'static str {
    match kind {
        UnusedKind::Variable => "variable",
        UnusedKind::Parameter => "parameter",
        UnusedKind::CatchParameter => "catchParameter",
        UnusedKind::Import => "import",
        UnusedKind::Type => "type",
        UnusedKind::Class => "class",
        UnusedKind::Function => "function",
        UnusedKind::Enum => "enum",
        UnusedKind::Other => "other",
    }
}
