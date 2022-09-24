// Example version strings:
// openjdk version "16.0.2" 2021-07-20
// openjdk version "11.0.12" 2021-07-20
// openjdk version "1.8.0_332"

// #[derive(Debug, Clone, PartialEq, Eq)]
// pub struct JavaVersion(pub String);
//
// pub async fn check_java_version() -> Result<JavaVersion, JavaError> {
//     let mut command = Command::new("java");
//     command.args(["-version"]);
//     let output = command.output().await
//         .map_err(|_| JavaError::MissingJava);
//
//
// }
