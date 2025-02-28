use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct CodeGen {
    inputs: Vec<PathBuf>,
    output_dir: PathBuf,
    protoc_path: Option<PathBuf>,
    protoc_gen_upb_minitable_path: Option<PathBuf>,
    includes: Vec<PathBuf>,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

impl CodeGen {
    pub fn new() -> Self {
        Self {
            inputs: Vec::new(),
            output_dir: PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("protobuf_generated"),
            protoc_path: None,
            protoc_gen_upb_minitable_path: None,
            includes: Vec::new(),
        }
    }

    pub fn input(&mut self, input: impl AsRef<Path>) -> &mut Self {
        self.inputs.push(input.as_ref().to_owned());
        self
    }

    pub fn inputs(&mut self, inputs: impl IntoIterator<Item = impl AsRef<Path>>) -> &mut Self {
        self.inputs.extend(inputs.into_iter().map(|input| input.as_ref().to_owned()));
        self
    }

    pub fn output_dir(&mut self, output_dir: impl AsRef<Path>) -> &mut Self {
        self.output_dir = output_dir.as_ref().to_owned();
        self
    }

    pub fn protoc_path(&mut self, protoc_path: impl AsRef<Path>) -> &mut Self {
        self.protoc_path = Some(protoc_path.as_ref().to_owned());
        self
    }

    pub fn protoc_gen_upb_minitable_path(
        &mut self,
        protoc_gen_upb_minitable_path: impl AsRef<Path>,
    ) -> &mut Self {
        self.protoc_gen_upb_minitable_path =
            Some(protoc_gen_upb_minitable_path.as_ref().to_owned());
        self
    }

    pub fn include(&mut self, include: impl AsRef<Path>) -> &mut Self {
        self.includes.push(include.as_ref().to_owned());
        self
    }

    pub fn includes(&mut self, includes: impl Iterator<Item = impl AsRef<Path>>) -> &mut Self {
        self.includes.extend(includes.into_iter().map(|include| include.as_ref().to_owned()));
        self
    }

    fn expected_generated_rs_files(&self) -> Vec<PathBuf> {
        self.inputs
            .iter()
            .map(|input| {
                let mut input = input.clone();
                assert!(input.set_extension("u.pb.rs"));
                self.output_dir.join(input)
            })
            .collect()
    }

    fn expected_generated_c_files(&self) -> Vec<PathBuf> {
        self.inputs
            .iter()
            .map(|input| {
                let mut input = input.clone();
                assert!(input.set_extension("upb_minitable.c"));
                self.output_dir.join(input)
            })
            .collect()
    }

    pub fn generate_and_compile(&self) -> Result<(), String> {
        let upb_version = std::env::var("DEP_UPB_VERSION").expect("DEP_UPB_VERSION should have been set, make sure that the Protobuf crate is a dependency");
        if VERSION != upb_version {
            panic!(
                "protobuf-codegen version {} does not match protobuf version {}.",
                VERSION, upb_version
            );
        }

        let protoc_path = if let Some(path) = &self.protoc_path {
            path.clone()
        } else {
            protoc_path().expect("To be a supported platform")
        };
        let mut cmd = std::process::Command::new(protoc_path);
        for input in &self.inputs {
            cmd.arg(input);
        }
        if !self.output_dir.exists() {
            // Attempt to make the directory if it doesn't exist
            let _ = std::fs::create_dir(&self.output_dir);
        }
        let protoc_gen_upb_minitable_path = if let Some(path) = &self.protoc_gen_upb_minitable_path
        {
            path.clone()
        } else {
            protoc_gen_upb_minitable_path().expect("To be a supported platform")
        };

        for include in &self.includes {
            println!("cargo:rerun-if-changed={}", include.display());
        }

        cmd.arg(format!("--rust_out={}", self.output_dir.display()))
            .arg("--rust_opt=experimental-codegen=enabled,kernel=upb")
            .arg(format!(
                "--plugin=protoc-gen-upb_minitable={}",
                protoc_gen_upb_minitable_path.display()
            ))
            .arg(format!("--upb_minitable_out={}", self.output_dir.display()));
        for include in &self.includes {
            cmd.arg(format!("--proto_path={}", include.display()));
        }
        let output = cmd.output().map_err(|e| format!("failed to run protoc: {}", e))?;
        println!("{}", std::str::from_utf8(&output.stdout).unwrap());
        eprintln!("{}", std::str::from_utf8(&output.stderr).unwrap());
        assert!(output.status.success());
        self.compile_only()
    }

    /// Builds and links the C code.
    pub fn compile_only(&self) -> Result<(), String> {
        let mut cc_build = cc::Build::new();
        cc_build
            .include(
                std::env::var_os("DEP_UPB_INCLUDE")
                    .expect("DEP_UPB_INCLUDE should have been set, make sure that the Protobuf crate is a dependency"),
            )
            .include(self.output_dir.clone())
            .flag("-std=c99");

        for path in &self.expected_generated_rs_files() {
            if !path.exists() {
                return Err(format!("expected generated file {} does not exist", path.display()));
            }
            println!("cargo:rerun-if-changed={}", path.display());
        }
        for path in &self.expected_generated_c_files() {
            if !path.exists() {
                return Err(format!("expected generated file {} does not exist", path.display()));
            }
            println!("cargo:rerun-if-changed={}", path.display());
            cc_build.file(path);
        }
        cc_build.compile(&format!("{}_upb_gen_code", std::env::var("CARGO_PKG_NAME").unwrap()));
        Ok(())
    }
}

fn get_path_for_arch() -> Option<PathBuf> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("bin");
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "x86_64") => path.push("osx-x86_64"),
        ("macos", "aarch64") => path.push("osx-aarch_64"),
        ("linux", "aarch64") => path.push("linux-aarch_64"),
        ("linux", "powerpc64") => path.push("linux-ppcle_64"),
        ("linux", "s390x") => path.push("linux-s390_64"),
        ("linux", "x86") => path.push("linux-x86_32"),
        ("linux", "x86_64") => path.push("linux-x86_64"),
        ("windows", "x86") => path.push("win32"),
        ("windows", "x86_64") => path.push("win64"),
        _ => return None,
    };
    Some(path)
}

pub fn protoc_path() -> Option<PathBuf> {
    let mut path = get_path_for_arch()?;
    path.push("protoc");
    Some(path)
}

pub fn protoc_gen_upb_minitable_path() -> Option<PathBuf> {
    let mut path = get_path_for_arch()?;
    path.push("protoc-gen-upb_minitable");
    Some(path)
}
