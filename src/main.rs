use std::io::Read;

use anyhow::Context;

#[derive(clap::Parser)]
struct Opt {
    #[arg(short, long)]
    spec_file: std::path::PathBuf,

    #[arg(short, long, value_name = "PATH")]
    delivery_folder: Option<std::path::PathBuf>,

    #[arg(short, long, value_name = "PATH")]
    grader_folder: Option<std::path::PathBuf>,

    #[arg(short, long)]
    login: String,
}

#[derive(serde::Deserialize)]
struct ProjectSpec {
    #[serde(flatten)]
    meta: ProjectMeta,
    tasks: Vec<ProjectTask>,
}

impl ProjectSpec {
    fn from_file_path(fp: &std::path::Path) -> anyhow::Result<Self> {
        let mut file = std::fs::File::open(fp).context("while opening project spec file")?;
        let mut contents = vec![];
        file.read_to_end(&mut contents)?;
        Ok(toml::from_slice(&contents)?)
    }
}

#[derive(serde::Deserialize)]
struct ProjectMeta {
    name: String,
    code_name: String,
    delivery_folder: std::path::PathBuf,
    grader_folder: std::path::PathBuf,
    compile_args_base: Vec<String>,
    compile_args_tail: Vec<String>,
}

#[derive(serde::Deserialize)]
struct ProjectTask {
    name: String,
    mandatory_files: Vec<std::path::PathBuf>,
    optional_files: Vec<std::path::PathBuf>,
    test_files: Vec<std::path::PathBuf>,
}

impl ProjectTask {
    fn are_garding_files_present(&self) -> bool {
        !self.test_files.iter().any(|item| !item.is_file())
    }
}

struct GradingOption<'a> {
    task: &'a ProjectTask,
    project: &'a ProjectMeta,
    repo: std::path::PathBuf,
}

impl<'a> GradingOption<'a> {
    fn new(task: &'a ProjectTask, project: &'a ProjectMeta, login: &str) -> anyhow::Result<Self> {
        let mut repo = project.delivery_folder.clone();
        repo.push(login);
        repo.push(&project.code_name);
        dbg!(&repo);
        repo.canonicalize().context("no delivery")?;
        Ok(Self {
            task,
            project,
            repo,
        })
    }

    fn grade(&self) -> anyhow::Result<()> {
        println!("testing {}", self.task.name);
        // check that all files mandatory in tasks are present
        // add to list optional files that are present
        // add to list the test files
        let mut files: Vec<_> = self
            .task
            .mandatory_files
            .iter()
            .map(|item| {
                let mut item_absolute = self.repo.clone();
                item_absolute.push(item);
                item_absolute
            })
            .collect();
        let mut missing_mandatory = files.iter().filter(|item| !item.is_file()).peekable();
        if let Some(_) = missing_mandatory.peek() {
            println!("missing mandatory files");
            missing_mandatory.for_each(|item| println!("- {}", item.display().to_string()));
            return Err(anyhow::anyhow!(
                "missing mandatory files for taks {}",
                self.task.name
            ));
        }
        self.task
            .optional_files
            .iter()
            .filter_map(|item| {
                let mut item_absolute = self.repo.clone();
                item_absolute.push(item);
                if item_absolute.is_file() {
                    Some(item_absolute)
                } else {
                    None
                }
            })
            .for_each(|item| files.push(item));
        self.task
            .test_files
            .iter()
            .map(|item| {
                let mut item_absolute = self.project.grader_folder.clone();
                item_absolute.push(item);
                item_absolute
            })
            .for_each(|item| files.push(item));
        dbg!(&files);
        self.compile(&files)?;
        self.run_tests(&files)?;
        Ok(())
    }

    fn compile(&self, files: &[std::path::PathBuf]) -> anyhow::Result<()> {
        let mut cmd = std::process::Command::new("gcc");
        let mut runing_cmd = cmd
            .args(&self.project.compile_args_base)
            .args(files)
            .args(&self.project.compile_args_tail)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .context("while spawning child")?;
        runing_cmd.wait().context("while awaiting child")?;
        Ok(())
    }

    fn run_tests(&self, files: &[std::path::PathBuf]) -> anyhow::Result<()> {
        let mut cmd = std::process::Command::new("./a.out");
        let mut runing_cmd = cmd
            .arg("--verbose")
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .context("while spawning child")?;
        runing_cmd.wait().context("while awaiting child")?;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let opt: Opt = clap::Parser::parse();
    let mut spec = ProjectSpec::from_file_path(&opt.spec_file).context("creating project spec")?;
    if let Some(delivery) = opt.delivery_folder {
        spec.meta.delivery_folder = delivery;
    }
    if let Some(grader) = opt.grader_folder {
        spec.meta.grader_folder = grader;
    }
    let mut grad_opt =
        GradingOption::new(&spec.tasks[0], &spec.meta, "michel").context("building grader")?;
    grad_opt.grade()?;
    grad_opt.task = &spec.tasks[1];
    grad_opt.grade()?;
    Ok(())
}
