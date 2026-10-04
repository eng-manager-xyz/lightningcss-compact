//! Build-time CSS factoring and class compression. Author styles in ordinary CSS.
//! A complete, explicit class inventory is required for renaming or atom expansion.
mod model;
mod optimize;
mod selectors;
mod size;
pub use lightningcss;
pub use model::*;
pub use selectors::discover_classes;
pub use size::encoded_size;

use lightningcss::{
    stylesheet::StyleSheet,
    visitor::{Visit, VisitTypes, Visitor},
};
use static_self::IntoOwned;

pub fn prepare_project(input: ProjectInput, options: Options) -> Result<PreparedProject, Error> {
    let (compiled, planned_sheets) = optimize::compile(&input, &options)?;
    Ok(PreparedProject {
        input,
        compiled,
        planned_sheets,
    })
}

pub fn compile_project(input: ProjectInput, options: Options) -> Result<CompiledProject, Error> {
    Ok(prepare_project(input, options)?.compiled)
}

/// Apply an immutable whole-project plan to a parsed stylesheet through the
/// native Lightning CSS visitor interface. Refuse different source/AST input.
pub fn apply_plan<'i>(
    project: &PreparedProject,
    stylesheet_id: &str,
    stylesheet: &mut StyleSheet<'i>,
) -> Result<TransformationReport, Error> {
    let source = project
        .input
        .stylesheets
        .iter()
        .find(|s| s.id == stylesheet_id)
        .ok_or_else(|| Error::Inventory(format!("unknown stylesheet {stylesheet_id}")))?;
    let expected = selectors::canonical_css(&source.source, &source.id)?;
    let actual = selectors::print_css(stylesheet, &source.id, false)?;
    if actual != expected {
        return Err(Error::Inventory(format!(
            "stylesheet {stylesheet_id} changed after project preparation"
        )));
    }
    struct Apply<'a> {
        planned: &'a StyleSheet<'static>,
    }
    impl<'i> Visitor<'i> for Apply<'_> {
        type Error = Error;
        fn visit_types(&self) -> VisitTypes {
            VisitTypes::RULES
        }
        fn visit_stylesheet(&mut self, sheet: &mut StyleSheet<'i>) -> Result<(), Error> {
            sheet.rules = self.planned.rules.clone().into_owned();
            Ok(())
        }
    }
    stylesheet.visit(&mut Apply {
        planned: &project.planned_sheets[stylesheet_id],
    })?;
    Ok(project.compiled.report.clone())
}
