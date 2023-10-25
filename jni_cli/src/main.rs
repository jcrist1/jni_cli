use std::{collections::HashMap, fs, path::Path, process::Stdio};

use cargo_metadata::MetadataCommand;
use color_eyre::eyre::{self};
use jni_cli_core::token_processing::{fill_lookup, KotlinClass, PackageLookup};

const GRADLE_WRAPPER_TEMPLATE: &[u8] = include_bytes!("template/gradle/wrapper/gradle-wrapper.jar");
const GRADLE_WRAPPER_PROPERTIES_TEMPLATE: &[u8] =
    include_bytes!("template/gradle/wrapper/gradle-wrapper.properties");
const GRADLE_BUILD_TEMPLATE: &str = include_str!("template/build.gradle.kts");
const GRADLE_SETTINGS_TEMPLATE: &str = include_str!("template/settings.gradle.kts");
const GRADLE_PROPERTIES_TEMPLATE: &[u8] = include_bytes!("template/gradle.properties");
const GRADLEW_BAT_TEMPLATE: &[u8] = include_bytes!("template/gradlew.bat");
const GRADLEW_TEMPLATE: &[u8] = include_bytes!("template/gradlew");
use clap::Parser;
use tokio::process::Command;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the group e.g. org.apache
    #[arg(short, long)]
    group: String,
    /// Name of the package e.g. commons-io
    #[arg(short, long)]
    package: String,
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let Args { group, package } = Args::parse();

    let mut cmd = MetadataCommand::new();
    cmd.manifest_path("Cargo.toml");
    let metadata = tokio::task::spawn_blocking(move || cmd.exec()).await??;
    let rust_package = metadata
        .root_package()
        .ok_or_else(|| eyre::eyre!("No package found"))?;
    let rust_lib = rust_package.name.clone();
    let reg = handlebars::Handlebars::new();
    let package_name = package.as_str();
    let project_root = format!("{group}.{package_name}");
    let group_dir = group.replace('.', "/");
    fs::create_dir_all(format!("kotlin/src/main/kotlin/{group_dir}/{package_name}"))?;
    fs::create_dir_all("kotlin/src/main/resources")?;
    fs::create_dir_all("kotlin/gradle/wrapper")?;
    let gradle_settings = reg.render_template(
        GRADLE_SETTINGS_TEMPLATE,
        &serde_json::json!({"package_name": package_name}),
    )?;
    fs::write("kotlin/settings.gradle.kts", gradle_settings)?;
    fs::write(
        "kotlin/gradle/wrapper/gradle-wrapper.jar",
        GRADLE_WRAPPER_TEMPLATE,
    )?;
    fs::write(
        "kotlin/gradle/wrapper/gradle-wrapper.properties",
        GRADLE_WRAPPER_PROPERTIES_TEMPLATE,
    )?;
    let gradle_build = reg.render_template(
        GRADLE_BUILD_TEMPLATE,
        &serde_json::json!({"package_name": package_name, "group_id": group}),
    )?;
    fs::write("kotlin/build.gradle.kts", gradle_build)?;

    fs::write("kotlin/gradle.properties", GRADLE_PROPERTIES_TEMPLATE)?;

    fs::write("kotlin/gradlew.bat", GRADLEW_BAT_TEMPLATE)?;
    fs::write("kotlin/gradlew", GRADLEW_TEMPLATE)?;

    let mut java_class_lookup: PackageLookup = HashMap::new();

    // create lookups for structs to package.Class
    for file in walkdir::WalkDir::new("src") {
        let file = file?;
        let name = file.file_name();
        let name_str = name
            .to_str()
            .ok_or(color_eyre::eyre::eyre!("Failed to convert filename to str"))?;
        let end = name_str
            .chars()
            .skip(name_str.len() - 3)
            .collect::<String>();
        if &end == ".rs" {
            let path = file.path();
            println!("{path:?}");
            let rust = fs::read_to_string(file.path())?;
            fill_lookup(&rust, &mut java_class_lookup)?;
        }
    }

    // create Kotlin files
    for file in walkdir::WalkDir::new("src") {
        let file = file?;
        let name = file.file_name();
        let name_str = name
            .to_str()
            .ok_or(color_eyre::eyre::eyre!("Failed to convert filename to str"))?;
        let end = name_str
            .chars()
            .skip(name_str.len() - 3)
            .collect::<String>();
        if &end == ".rs" {
            let path = file.path();
            println!("{path:?}");
            let rust = fs::read_to_string(file.path())?;
            let impls = jni_cli_core::token_processing::rust_file_to_tokens(
                &project_root,
                &rust,
                &java_class_lookup,
                &rust_lib,
            )?;
            for KotlinClass { path, name, code } in impls {
                let path = path.replace('.', "/");
                let file_dir = format!("kotlin/src/main/kotlin/{path}");
                fs::create_dir_all(&file_dir)?;
                fs::write(format!("{file_dir}/{name}.kt"), code)?;
            }
        }
    }

    // create top-level file
    fs::write(
        format!(
            "kotlin/src/main/kotlin/{}/Library.kt",
            project_root.replace('.', "/")
        ),
        format!(
            r#"
package {project_root}


object Library {{
    val CLEANER = 
        java.lang.ref.Cleaner.create()
}}
               "#,
        ),
    )?;

    // compile rust artifact
    let path = Path::new("cargo");
    let args = ["build", "--release"];
    Command::new(path)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?
        .wait()
        .await?;

    // move rust-artifact
    println!("Name: lib{rust_lib}.dylib");
    fs::copy(
        // todo change to be the actual location
        format!("../target/release/lib{rust_lib}.dylib"),
        format!("kotlin/src/main/resources/lib{rust_lib}.dylib"),
    )?;
    Ok(())
}

#[cfg(test)]
mod depth;

#[cfg(test)]
mod test {
    use crate::depth::boop::SomeStruct;
    use jni_cli_macro::java_class;

    pub struct SomeStruct2;

    #[java_class("beep.boop")]
    impl SomeStruct2 {
        fn newFrom(_s: String, _idx: i32) -> SomeStruct2 {
            SomeStruct2
        }

        fn do_more_stuff(&self, string: String) -> i64 {
            string.len() as i64
        }

        fn do_more_even_more_stuff(&self, _string: String) -> SomeStruct {
            SomeStruct
        }
    }
}
