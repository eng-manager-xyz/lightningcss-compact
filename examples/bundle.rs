use lightningcss_compact::{
    compile_project, BindingInput, BindingKind, Options, ProjectInput, StylesheetInput,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let compiled=compile_project(ProjectInput {
        stylesheets: vec![StylesheetInput { id:"cards.css".into(), source:".left{color:red;height:100px}.right{color:red;height:100px}".into() }],
        bindings:vec![BindingInput { id:"document".into(),kind:BindingKind::Html,value:"<main><div data-probe=\"left\" class=\"left\">left</div><div data-probe=\"right\" class=\"right\">right</div></main>".into() }],
        managed_classes:["left".into(),"right".into()].into(),complete_usage:true,
        load_groups:vec![vec!["cards.css".into()]],..ProjectInput::default()
    },Options::default())?;
    println!("{}", compiled.stylesheets["cards.css"]);
    println!("{}", compiled.bindings["document"]);
    println!("generation: {}", compiled.manifest.generation);
    Ok(())
}
