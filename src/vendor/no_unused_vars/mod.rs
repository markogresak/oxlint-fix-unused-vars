// Synced from oxc @ 14533a3dc118bea73e755426aaf35f71dbe81eb8: crates/oxc_linter/src/rules/eslint/no_unused_vars/mod.rs

mod allowed;
mod binding_pattern;
mod ignored;
mod options;
mod symbol;
mod usage;

use std::ops::Deref;

use allowed::FunctionParameterKind;
use oxc_ast::AstKind;
use oxc_semantic::{ScopeFlags, Semantic, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};
use oxc_syntax::module_record::ModuleRecord;
use symbol::Symbol;

pub use options::{ArgsOption, CaughtErrors, IgnorePattern, NoUnusedVarsOptions, VarsOption};

#[derive(Debug, Clone)]
pub struct UnusedBinding {
    pub symbol_id: SymbolId,
    pub name: String,
    pub span: Span,
    pub kind: UnusedKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnusedKind {
    Variable,
    Parameter,
    CatchParameter,
    Import,
    Type,
    Class,
    Function,
    Enum,
    Other,
}

#[derive(Debug, Clone)]
pub(crate) struct NoUnusedVars(NoUnusedVarsOptions);

impl Deref for NoUnusedVars {
    type Target = NoUnusedVarsOptions;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub fn find_unused_bindings<'a>(
    semantic: &Semantic<'a>,
    module_record: &ModuleRecord<'a>,
    options: &NoUnusedVarsOptions,
) -> Vec<UnusedBinding> {
    let detector = NoUnusedVars(options.clone());
    let exported_names = Symbol::collect_exported_local_names(module_record);
    let mut unused = Vec::new();

    for symbol_id in semantic.scoping().symbol_ids() {
        let symbol = Symbol::new(semantic, module_record, symbol_id);
        if NoUnusedVars::should_skip_symbol(&symbol) {
            continue;
        }

        detector.collect_symbol(&symbol, &exported_names, &mut unused);
    }

    unused
}

impl NoUnusedVars {
    fn collect_symbol<'a>(
        &self,
        symbol: &Symbol<'_, 'a>,
        exported_names: &rustc_hash::FxHashSet<&str>,
        unused: &mut Vec<UnusedBinding>,
    ) {
        if self.is_ignored(symbol).is_some()
            || symbol.is_exported(exported_names)
            || symbol.has_usages(self)
        {
            return;
        }

        let declaration = symbol.declaration();
        let kind = match declaration.kind() {
            AstKind::ImportDeclaration(_)
            | AstKind::ImportSpecifier(_)
            | AstKind::ImportExpression(_)
            | AstKind::ImportDefaultSpecifier(_)
            | AstKind::ImportNamespaceSpecifier(_) => UnusedKind::Import,
            AstKind::VariableDeclarator(decl) => {
                if self.is_allowed_variable_declaration(symbol, decl) {
                    return;
                }
                UnusedKind::Variable
            }
            AstKind::FormalParameter(param) => {
                if self.is_allowed_argument(
                    symbol.semantic(),
                    symbol.module_record(),
                    symbol,
                    &FunctionParameterKind::Normal(param),
                ) {
                    return;
                }
                UnusedKind::Parameter
            }
            AstKind::FormalParameterRest(param) => {
                if self.is_allowed_argument(
                    symbol.semantic(),
                    symbol.module_record(),
                    symbol,
                    &FunctionParameterKind::Rest(param),
                ) {
                    return;
                }
                UnusedKind::Parameter
            }
            AstKind::BindingRestElement(_) => {
                if Self::is_allowed_binding_rest_element(symbol) {
                    return;
                }
                UnusedKind::Variable
            }
            AstKind::TSModuleDeclaration(namespace) => {
                if self.is_allowed_ts_namespace(symbol, namespace) {
                    return;
                }
                UnusedKind::Type
            }
            AstKind::TSInterfaceDeclaration(_) | AstKind::TSTypeAliasDeclaration(_) => {
                if symbol.is_in_declared_module() {
                    return;
                }
                UnusedKind::Type
            }
            AstKind::TSTypeParameter(_) => {
                if self.is_allowed_type_parameter(symbol, declaration.id()) {
                    return;
                }
                UnusedKind::Type
            }
            AstKind::TSMappedType(_) => return,
            AstKind::CatchParameter(_) => UnusedKind::CatchParameter,
            AstKind::Class(_) => UnusedKind::Class,
            AstKind::Function(_) => UnusedKind::Function,
            AstKind::TSEnumDeclaration(_) => UnusedKind::Enum,
            _ if symbol.flags().is_type() => UnusedKind::Type,
            _ => UnusedKind::Other,
        };

        unused.push(UnusedBinding {
            symbol_id: symbol.id(),
            name: symbol.name().to_owned(),
            span: symbol.span(),
            kind,
        });
    }

    fn should_skip_symbol(symbol: &Symbol<'_, '_>) -> bool {
        const AMBIENT_NAMESPACE_FLAGS: SymbolFlags =
            SymbolFlags::NamespaceModule.union(SymbolFlags::Ambient);
        let flags = symbol.flags();

        if flags.intersects(SymbolFlags::EnumMember)
            || flags == AMBIENT_NAMESPACE_FLAGS
            || (symbol.is_in_ts() && symbol.is_in_declare_global())
        {
            return true;
        }

        let node_id = symbol.declaration().id();
        if flags.intersects(SymbolFlags::FunctionScopedVariable) {
            if let AstKind::FormalParameters(formal_parameters) =
                symbol.nodes().parent_node(node_id).kind()
            {
                if formal_parameters.kind.is_signature() {
                    return true;
                }
            }
        }

        if flags.contains(SymbolFlags::Import)
            && symbol.is_in_jsx()
            && symbol.is_possibly_jsx_factory()
        {
            return true;
        }

        false
    }
}

impl Symbol<'_, '_> {
    #[inline]
    fn is_possibly_jsx_factory(&self) -> bool {
        matches!(self.name(), "React" | "h")
    }

    fn is_in_declare_global(&self) -> bool {
        self.scoping()
            .scope_ancestors(self.scope_id())
            .filter(|&scope_id| {
                self.scoping()
                    .scope_flags(scope_id)
                    .contains(ScopeFlags::TsModuleBlock)
            })
            .any(|scope_id| {
                matches!(
                    self.nodes()
                        .get_node(self.scoping().get_node_id(scope_id))
                        .kind(),
                    AstKind::TSGlobalDeclaration(_)
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;

    use super::{find_unused_bindings, NoUnusedVarsOptions, UnusedKind};

    #[test]
    fn finds_unused_const() {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, "const unused = 1;", SourceType::default()).parse();
        assert!(parsed.diagnostics.is_empty());

        let semantic = SemanticBuilder::new_compiler()
            .with_build_nodes(true)
            .build(&parsed.program);
        assert!(semantic.diagnostics.is_empty());

        let unused = find_unused_bindings(
            &semantic.semantic,
            &parsed.module_record,
            &NoUnusedVarsOptions::default(),
        );
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].name, "unused");
        assert_eq!(unused[0].kind, UnusedKind::Variable);
    }

}
