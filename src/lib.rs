use std::process::Stdio;

use bstr::ByteSlice;
pub use internal_flake_show_output::FlakeInfo;
pub use internal_flake_show_output::IndividualFlakeInfos;

pub fn nix_cmd() -> std::process::Command {
    std::process::Command::new("/nix/var/nix/profiles/default/bin/nix")
}

#[derive(Debug)]
pub enum NixFlakeLogFormat {
    Raw,
    InternalJson,
    Bar,
    BarWithLogs,
}

#[derive(Default, Debug)]
pub struct NixFlakeShowBuilder {
    all_systems: bool,
    json: bool,
    legacy: bool,
    impure: bool,
    recreate_lock_file: bool,
    debug: bool,
    verbosity_level: usize,
    log_format: Option<NixFlakeLogFormat>,
    url: Option<std::path::PathBuf>,
}

impl NixFlakeShowBuilder {
    pub fn all_systems(mut self, all_systems: bool) -> Self {
        self.all_systems = all_systems;
        self
    }

    pub fn json(mut self, json: bool) -> Self {
        self.json = json;
        self
    }

    pub fn legacy(mut self, legacy: bool) -> Self {
        self.legacy = legacy;
        self
    }

    pub fn impure(mut self, impure: bool) -> Self {
        self.impure = impure;
        self
    }

    pub fn recreate_lock_file(mut self, recreate_lock_file: bool) -> Self {
        self.recreate_lock_file = recreate_lock_file;
        self
    }

    pub fn debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    pub fn verbosity_level(mut self, verbosity_level: usize) -> Self {
        self.verbosity_level = verbosity_level;
        self
    }

    pub fn log_format(mut self, log_format: Option<NixFlakeLogFormat>) -> Self {
        self.log_format = log_format;
        self
    }

    pub fn url(mut self, url: std::path::PathBuf) -> Self {
        self.url = Some(url);
        self
    }

    pub fn into_structured(self) -> Result<Option<FlakeInfo>, std::io::Error> {
        let output = self.build().output()?;

        if output.status.success() {
            Ok(Some(FlakeInfo::from_stdout(&output.stdout)))
        } else {
            Ok(None)
        }
    }

    pub fn build(self) -> std::process::Command {
        let mut cmd = nix_cmd();
        cmd.arg("flake").arg("show");

        if let Some(url) = self.url {
            cmd.arg(url);
        }

        if self.all_systems {
            cmd.arg("--all-systems");
        }

        if self.json {
            cmd.arg("--json");
        }

        if self.legacy {
            cmd.arg("--legacy");
        }

        if self.impure {
            cmd.arg("--impure");
        }

        if self.recreate_lock_file {
            cmd.arg("--recreate-lock-file");
        }

        if self.debug {
            cmd.arg("--debug");
        }

        if self.verbosity_level > 0 {
            cmd.arg(format!("-{}", "v".repeat(self.verbosity_level)));
        }

        if let Some(log_format) = self.log_format {
            cmd.arg("--log-format");
            let log_format_str = match log_format {
                NixFlakeLogFormat::Raw => "raw",
                NixFlakeLogFormat::InternalJson => "internal-json",
                NixFlakeLogFormat::Bar => "bar",
                NixFlakeLogFormat::BarWithLogs => "bar-with-logs",
            };
            cmd.arg(log_format_str);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit());
        cmd
    }
}
pub use internal_flake_show_output::Derivation;

mod internal_flake_show_output {
    use std::collections::{HashMap, HashSet};

    use serde::{Deserialize, Serialize};

    use crate::current_nix_system;

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct FlakeShowOutput {
        // from architecture to named fields
        #[serde(default)]
        dev_shells: HashMap<String, HighLevelFieldAnatomy>,
        #[serde(default)]
        packages: HashMap<String, HighLevelFieldAnatomy>,
    }

    #[derive(Serialize, Deserialize)]
    pub struct HighLevelFieldAnatomy {
        #[serde(flatten)]
        // from named fields to derivation details
        names: HashMap<String, FlakeAnatomyDetail>,
    }
    #[derive(Serialize, Deserialize)]
    pub struct FlakeAnatomyDetail {
        pub name: String,
        pub r#type: String,
        pub description: Option<String>,
    }

    #[derive(Debug, Clone)]
    pub struct Derivation {
        pub name: String,
        pub kind: String,
        pub description: Option<String>,
        pub invocation: String,
    }

    #[derive(Debug, Clone)]
    pub struct IndividualFlakeInfos {
        pub dev_shells: Vec<Derivation>,
        pub packages: Vec<Derivation>,
    }

    #[derive(Debug, Clone)]
    pub struct FlakeInfo {
        // from architecture to derivation
        pub dev_shells: HashMap<String, Vec<Derivation>>,
        pub packages: HashMap<String, Vec<Derivation>>,
    }

    impl FlakeInfo {
        pub fn for_system(&self, sys: &str) -> IndividualFlakeInfos {
            IndividualFlakeInfos {
                dev_shells: self.dev_shells.get(sys).cloned().unwrap_or_default(),
                packages: self.packages.get(sys).cloned().unwrap_or_default(),
            }
        }
        pub fn for_current_system(&self) -> IndividualFlakeInfos {
            self.for_system(&current_nix_system())
        }

        pub fn from_stdout(v: &[u8]) -> Self {
            serde_json::from_slice::<FlakeShowOutput>(v).unwrap().into()
        }
    }

    impl From<FlakeShowOutput> for FlakeInfo {
        fn from(value: FlakeShowOutput) -> Self {
            let mut devs = HashMap::new();
            for (arch, anat) in value.dev_shells {
                let derivs: &mut Vec<Derivation> = devs.entry(arch).or_default();
                for (invok, details) in anat.names {
                    derivs.push(Derivation {
                        name: details.name,
                        kind: details.r#type,
                        description: details.description,
                        invocation: invok,
                    });
                }
            }

            let mut packages = HashMap::new();
            for (arch, anat) in value.packages {
                let derivs: &mut Vec<Derivation> = packages.entry(arch).or_default();
                for (invok, details) in anat.names {
                    derivs.push(Derivation {
                        name: details.name,
                        kind: details.r#type,
                        description: details.description,
                        invocation: invok,
                    });
                }
            }

            FlakeInfo {
                dev_shells: devs,
                packages,
            }
        }
    }
}

pub fn current_nix_system() -> String {
    let mut cmd = nix_cmd();
    cmd.args(&[
        "eval",
        "--impure",
        "--raw",
        "--expr",
        "builtins.currentSystem",
    ]);

    cmd.output().unwrap().stdout.as_bstr().to_string()
}
pub fn flake_show() -> NixFlakeShowBuilder {
    NixFlakeShowBuilder::default()
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn it_works() {
        let structured = flake_show()
            .url("/Users/andy/Documents/colby/jerzy-work/surveyConformal".into())
            .json(true)
            .all_systems(true)
            .into_structured();

        dbg!(structured.unwrap().unwrap().for_current_system());
    }
}
