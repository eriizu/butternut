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
    // TODO: add a path for the produced binary
}

impl<'a> GradingOption<'a> {
    fn new(task: &'a ProjectTask, project: &'a ProjectMeta, login: &str) -> anyhow::Result<Self> {
        let mut repo = project.delivery_folder.clone();
        repo.push(login);
        repo.push(&project.code_name);
        repo.canonicalize().context("no delivery")?;
        Ok(Self {
            task,
            project,
            repo,
        })
    }

    fn grade(&self) -> anyhow::Result<()> {
        println!("testing {} of {}", self.repo.display(), self.task.name);
        let mandatory_files =
            Self::mk_file_list(&self.task.mandatory_files, self.repo.as_path(), false);
        let mut missing_mandatory = mandatory_files
            .iter()
            .filter(|item| !item.is_file())
            .peekable();
        if let Some(_) = missing_mandatory.peek() {
            // TODO: missing mandaroty printing should not be here, there should be in an error
            // with its own display implementation
            println!("missing mandatory files:");
            missing_mandatory.for_each(|item| println!("- {}", item.display().to_string()));
            return Err(anyhow::anyhow!(
                "missing mandatory files for taks {}",
                self.task.name
            ));
        }
        let optional = Self::mk_file_list(&self.task.optional_files, self.repo.as_path(), true);
        let test_files = Self::mk_file_list(
            &self.task.test_files,
            self.project.grader_folder.as_path(),
            false,
        );

        let without_optional = [mandatory_files.as_slice(), test_files.as_slice()].concat();
        dbg!(&without_optional);
        if let Err(err) = self.compile(&without_optional) {
            eprintln!("GradingOption::grade: without optional: {err}");
            if optional.is_empty() {
                // TODO: this should be an error variant
                eprintln!("no optional files, not trying again");
                return Err(err);
            }
            let with_optional = [
                mandatory_files.as_slice(),
                optional.as_slice(),
                test_files.as_slice(),
            ]
            .concat();
            dbg!(&with_optional);
            self.compile(&with_optional)?;
        };
        self.run_tests()?;
        // TODO: cleanup binary after test
        Ok(())
    }

    fn mk_file_list(
        file_names: &[std::path::PathBuf],
        root: &std::path::Path,
        prune_missing: bool,
    ) -> Vec<std::path::PathBuf> {
        file_names
            .iter()
            .filter_map(|item| {
                let mut item_absolute = root.to_owned();
                item_absolute.push(item);
                if !prune_missing || item_absolute.is_file() {
                    Some(item_absolute)
                } else {
                    None
                }
            })
            .collect()
    }

    // TODO: possible issues for error enum
    // - process spawning error
    // - process waiting error
    // - status non-zero when compilation fails
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
        let status = runing_cmd.wait().context("while awaiting child")?;
        if !status.success() {
            Err(anyhow::anyhow!("child process exited with non zero status"))
        } else {
            Ok(())
        }
    }

    // TODO: possible issues for error enum
    // - process spawning error
    // - process waiting error
    // - non-zero status when run failed
    fn run_tests(&self) -> anyhow::Result<()> {
        let mut cmd = std::process::Command::new("./a.out");
        let mut runing_cmd = cmd
            .arg("--verbose")
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .context("while spawning child")?;
        let status = runing_cmd.wait().context("while awaiting child")?;
        if !status.success() {
            Err(anyhow::anyhow!("child process exited with non zero status"))
        } else {
            Ok(())
        }
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
    for task in &spec.tasks {
        let grad_opt = GradingOption::new(task, &spec.meta, "michel").context("building grader")?;
        if let Err(_) = grad_opt.grade() {
            // eprintln!("{}", err);
        }
        print!("\n\n")
    }
    Ok(())
}
