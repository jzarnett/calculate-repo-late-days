use std::fs::File;
use std::io::{BufRead, BufReader, Lines, Write};
use std::time::Duration;
use std::{env, fs};

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use chrono_tz::Canada::Eastern;
use chrono_tz::Tz;
use gitlab::api::projects::repository::branches::BranchBuilder;
use gitlab::api::{projects, Query};
use gitlab::{Gitlab, ObjectId};
use serde::Deserialize;

const UW_GITLAB_URL: &str = "git.uwaterloo.ca";
const DEFAULT_BRANCH_NAME: &str = "main";
const DATE_TIME_FORMAT: &str = "%Y-%m-%d %H:%M";
const MINS_PER_DAY: f64 = 60.0 * 24.0;

#[derive(Debug, Deserialize)]
struct Project {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct Commit {
    id: ObjectId,
    committed_date: DateTime<FixedOffset>,
}

#[derive(Debug, Deserialize)]
struct Branch {
    default: bool,
    commit: Commit,
}

struct GitLabConfig {
    designation: String,
    starter_commit_hash: String,
    group_name: String,
    due_date_time: DateTime<Tz>,
    tolerance: Duration,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if !validate_args_len(&args) {
        return;
    }

    let config = build_config(&args);
    let repo_members = parse_csv_file(args.get(5).unwrap());

    let token = read_token_file(args.get(6).unwrap());
    let client = Gitlab::new(String::from(UW_GITLAB_URL), token).unwrap();

    get_late_days(client, repo_members, config)
}

fn validate_args_len(args: &Vec<String>) -> bool {
    if args.len() != 8 {
        println!(
            "Usage: {} <designation> <starter_commit_hash> <gitlab_group_name> <due_date_time> <tolerance_in_mins> <list_of_student_groups.csv> <token_file>",
            args.first().unwrap()
        );
        println!(
            "Example: {} a1 c335fdb690e88c7cd162e10d42800e93 ece459-1231 \"2023-01-27 23:59\" 60 students.csv token.git",
            args.first().unwrap()
        );
        return false;
    }
    true
}

fn build_config(args: &[String]) -> GitLabConfig {
    let duration_minutes: u64 = args.get(5).unwrap().parse().unwrap();
    let naive_date_time =
        NaiveDateTime::parse_from_str(args.get(4).unwrap(), DATE_TIME_FORMAT).unwrap();
    let due_date = naive_date_time.and_local_timezone(Eastern).unwrap();

    let config = GitLabConfig {
        designation: String::from(args.get(1).unwrap()),
        starter_commit_hash: String::from(args.get(2).unwrap()),
        group_name: String::from(args.get(3).unwrap()),
        due_date_time: due_date,
        tolerance: Duration::from_secs(60 * duration_minutes),
    };
    config
}

fn get_late_days(client: Gitlab, repo_members: Vec<Vec<String>>, config: GitLabConfig) {
    let output_file_name = format! {"{}-{}-latedays.csv", config.group_name, config.designation};
    let no_change_file_name = format! {"{}-{}-nochange.csv", config.group_name, config.designation};
    let mut output_file = File::create(output_file_name).unwrap();
    let mut no_change_file = File::create(no_change_file_name).unwrap();
    let effective_due_date = calculate_effective_due_date(config.due_date_time, config.tolerance);

    for i in 0..repo_members.len() {
        let group_or_student = repo_members.get(i).unwrap();
        let project_name = if group_or_student.len() == 1 {
            format!(
                "{}-{}-{}",
                config.group_name,
                config.designation,
                group_or_student.first().unwrap()
            )
        } else {
            format!("{}-{}-g{}", config.group_name, config.designation, (i + 1))
        };

        println!("Calculating late days for project {project_name}...");
        let last_commit = get_last_commit(
            &client,
            &config.group_name,
            &config.starter_commit_hash,
            &project_name,
        );
        if last_commit.is_none() {
            println!("Project {project_name} has not been changed since the starter commit hash.");
            for student in group_or_student {
                let no_change_line = format!("{student}\n");
                no_change_file.write_all(no_change_line.as_bytes()).unwrap();
            }
            continue;
        }
        let lateness_in_days = calculate_lateness(last_commit.unwrap(), effective_due_date);
        println!("Project {project_name} is submitted {lateness_in_days} day(s) late.");
        for student in group_or_student {
            let file_line = format!("{student},{lateness_in_days}\n");
            output_file.write_all(file_line.as_bytes()).unwrap();
        }
    }
}

fn calculate_effective_due_date(due_date_time: DateTime<Tz>, tolerance: Duration) -> DateTime<Tz> {
    due_date_time
        .checked_add_signed(chrono::Duration::from_std(tolerance).unwrap())
        .unwrap()
}

fn calculate_lateness(last_commit: DateTime<Tz>, due_date_time: DateTime<Tz>) -> i64 {
    if last_commit.le(&due_date_time) {
        return 0;
    }
    let diff = (last_commit - due_date_time).num_minutes();
    1 + (diff as f64 / MINS_PER_DAY).floor() as i64
}

fn get_last_commit(
    client: &Gitlab,
    group_name: &String,
    starter_commit_hash: &String,
    project_name: &String,
) -> Option<DateTime<Tz>> {
    let project_builder = projects::ProjectBuilder::default()
        .project(format!("{group_name}/{project_name}"))
        .build()
        .unwrap();

    let project: Project = project_builder.query(client).unwrap();
    let project_id = project.id;

    let branch_builder = BranchBuilder::default()
        .project(project_id)
        .branch(DEFAULT_BRANCH_NAME)
        .build()
        .unwrap();

    let branch: Branch = branch_builder.query(client).unwrap();
    if !branch.default {
        println!(
            "Project {project_name} uses a different default branch than expected {DEFAULT_BRANCH_NAME}!",
        )
    }
    if branch.commit.id.value() == starter_commit_hash {
        return None;
    }
    Some(branch.commit.committed_date.with_timezone(&Eastern))
}

fn parse_csv_file(filename: &String) -> Vec<Vec<String>> {
    let mut result: Vec<Vec<String>> = Vec::new();
    let lines = read_lines(filename);

    for line in lines {
        let line = line.unwrap();
        let mut inner = Vec::new();
        for user in line.split(',') {
            inner.push(String::from(user.trim()))
        }
        result.push(inner);
    }
    result
}

fn read_lines(filename: &String) -> Lines<BufReader<File>> {
    let file = File::open(filename).unwrap();
    BufReader::new(file).lines()
}

fn read_token_file(filename: &String) -> String {
    let mut token = fs::read_to_string(filename)
        .unwrap_or_else(|_| panic!("Unable to read token from file {filename}"));
    token.retain(|c| !c.is_whitespace());
    token
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::fs::{remove_file, File};
    use std::io::Write;
    use std::path::Path;
    use std::time::Duration;

    use chrono::NaiveDateTime;
    use chrono_tz::Canada::Eastern;
    use gitlab::Gitlab;

    use httpmock::prelude::*;

    use crate::{
        build_config, calculate_effective_due_date, calculate_lateness, get_last_commit,
        get_late_days, parse_csv_file, read_token_file, validate_args_len, GitLabConfig,
        DATE_TIME_FORMAT,
    };

    #[test]
    fn late_days_zero_if_sub_day_before_due_date() {
        let due_date = NaiveDateTime::parse_from_str("2023-01-24 22:05", DATE_TIME_FORMAT).unwrap();
        let due_date = due_date.and_local_timezone(Eastern).unwrap();

        let submit_date =
            NaiveDateTime::parse_from_str("2023-01-23 11:29", DATE_TIME_FORMAT).unwrap();
        let submit_date = submit_date.and_local_timezone(Eastern).unwrap();

        assert_eq!(calculate_lateness(submit_date, due_date), 0);
    }

    #[test]
    fn late_days_zero_if_sub_hours_before_due_date() {
        let due_date = NaiveDateTime::parse_from_str("2023-01-24 22:05", DATE_TIME_FORMAT).unwrap();
        let due_date = due_date.and_local_timezone(Eastern).unwrap();

        let submit_date =
            NaiveDateTime::parse_from_str("2023-01-24 11:29", DATE_TIME_FORMAT).unwrap();
        let submit_date = submit_date.and_local_timezone(Eastern).unwrap();

        assert_eq!(calculate_lateness(submit_date, due_date), 0);
    }

    #[test]
    fn late_days_zero_if_sub_at_due_date() {
        let due_date = NaiveDateTime::parse_from_str("2023-01-24 22:05", DATE_TIME_FORMAT).unwrap();
        let due_date = due_date.and_local_timezone(Eastern).unwrap();

        let submit_date =
            NaiveDateTime::parse_from_str("2023-01-24 22:05", DATE_TIME_FORMAT).unwrap();
        let submit_date = submit_date.and_local_timezone(Eastern).unwrap();

        assert_eq!(calculate_lateness(submit_date, due_date), 0);
    }

    #[test]
    fn late_days_one_if_sub_next_day() {
        let due_date = NaiveDateTime::parse_from_str("2023-01-24 22:05", DATE_TIME_FORMAT).unwrap();
        let due_date = due_date.and_local_timezone(Eastern).unwrap();

        let submit_date =
            NaiveDateTime::parse_from_str("2023-01-25 08:12", DATE_TIME_FORMAT).unwrap();
        let submit_date = submit_date.and_local_timezone(Eastern).unwrap();

        assert_eq!(calculate_lateness(submit_date, due_date), 1);
    }

    #[test]
    fn late_days_one_if_sub_1h_late() {
        let due_date = NaiveDateTime::parse_from_str("2023-01-24 22:05", DATE_TIME_FORMAT).unwrap();
        let due_date = due_date.and_local_timezone(Eastern).unwrap();

        let submit_date =
            NaiveDateTime::parse_from_str("2023-01-24 23:05", DATE_TIME_FORMAT).unwrap();
        let submit_date = submit_date.and_local_timezone(Eastern).unwrap();

        assert_eq!(calculate_lateness(submit_date, due_date), 1);
    }

    #[test]
    fn late_days_one_if_sub_5m_late() {
        let due_date = NaiveDateTime::parse_from_str("2023-01-24 22:05", DATE_TIME_FORMAT).unwrap();
        let due_date = due_date.and_local_timezone(Eastern).unwrap();

        let submit_date =
            NaiveDateTime::parse_from_str("2023-01-24 22:10", DATE_TIME_FORMAT).unwrap();
        let submit_date = submit_date.and_local_timezone(Eastern).unwrap();

        assert_eq!(calculate_lateness(submit_date, due_date), 1);
    }

    #[test]
    fn late_days_three_if_sub_over_2_days_late() {
        let due_date = NaiveDateTime::parse_from_str("2023-01-24 22:05", DATE_TIME_FORMAT).unwrap();
        let due_date = due_date.and_local_timezone(Eastern).unwrap();

        let submit_date =
            NaiveDateTime::parse_from_str("2023-01-26 23:50", DATE_TIME_FORMAT).unwrap();
        let submit_date = submit_date.and_local_timezone(Eastern).unwrap();

        assert_eq!(calculate_lateness(submit_date, due_date), 3);
    }

    #[test]
    fn successfully_read_token_file() {
        let token = "1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let file_name = "tmp_token1car.git";
        {
            let mut token_file = File::create(Path::new(file_name)).unwrap();
            token_file.write_all(token.as_bytes()).unwrap();
        } // Let it go out of scope so it's closed
        let filename = String::from(file_name);
        let read_token = read_token_file(&filename);
        remove_file(Path::new(file_name)).unwrap();
        assert_eq!(read_token, token);
    }

    #[test]
    fn token_is_trimmed_nicely() {
        let token = "1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let file_name = "tmp_token2.git";
        {
            let mut token_file = File::create(Path::new(file_name)).unwrap();
            token_file.write_all("  ".as_bytes()).unwrap();
            token_file.write_all(token.as_bytes()).unwrap();
            token_file.write_all("  \n".as_bytes()).unwrap();
        } // Let it go out of scope so it's closed
        let filename = String::from(file_name);
        let read_token = read_token_file(&filename);
        remove_file(Path::new(file_name)).unwrap();
        assert_eq!(read_token, token);
    }

    #[test]
    fn can_parse_simple_csv() {
        let test_filename = String::from("test/resources/simple.csv");
        let mut expected: Vec<Vec<String>> = Vec::new();
        let mut inner = Vec::new();
        inner.push(String::from("username"));
        expected.push(inner);

        let parsed = parse_csv_file(&test_filename);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn can_parse_group_csv() {
        let test_filename = String::from("test/resources/group.csv");
        let mut expected: Vec<Vec<String>> = Vec::new();
        let mut inner = Vec::new();
        inner.push(String::from("username"));
        inner.push(String::from("u2sernam"));
        inner.push(String::from("u3sernam"));
        expected.push(inner);

        let parsed = parse_csv_file(&test_filename);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn can_parse_group_w_spaces_csv() {
        let test_filename = String::from("test/resources/group_spaces.csv");
        let mut expected: Vec<Vec<String>> = Vec::new();
        let mut inner = Vec::new();
        inner.push(String::from("username"));
        inner.push(String::from("u2sernam"));
        inner.push(String::from("u3sernam"));
        expected.push(inner);

        let parsed = parse_csv_file(&test_filename);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn can_parse_multiple_csv() {
        let test_filename = String::from("test/resources/multiple.csv");
        let mut expected: Vec<Vec<String>> = Vec::new();
        let mut inner = Vec::new();
        inner.push(String::from("username"));
        expected.push(inner);
        let mut inner = Vec::new();
        inner.push(String::from("u2sernam"));
        expected.push(inner);
        let mut inner = Vec::new();
        inner.push(String::from("u3sernam"));
        expected.push(inner);

        let parsed = parse_csv_file(&test_filename);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn can_parse_with_newline_at_eof() {
        let test_filename = String::from("test/resources/newline_eof.csv");
        let mut expected: Vec<Vec<String>> = Vec::new();
        let mut inner = Vec::new();
        inner.push(String::from("username"));
        expected.push(inner);
        let mut inner = Vec::new();
        inner.push(String::from("u2sernam"));
        expected.push(inner);
        let mut inner = Vec::new();
        inner.push(String::from("u3sernam"));
        expected.push(inner);

        let parsed = parse_csv_file(&test_filename);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn can_parse_group_w_uneven_sizes_csv() {
        let test_filename = String::from("test/resources/group_uneven_sizes.csv");
        let mut expected: Vec<Vec<String>> = Vec::new();
        let mut inner = Vec::new();
        inner.push(String::from("username"));
        inner.push(String::from("u2sernam"));
        expected.push(inner);

        let mut inner = Vec::new();
        inner.push(String::from("u3sernam"));
        inner.push(String::from("u4sernam"));
        inner.push(String::from("u5sernam"));
        expected.push(inner);

        let parsed = parse_csv_file(&test_filename);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn can_parse_mixed_csv() {
        let test_filename = String::from("test/resources/mixed.csv");
        let mut expected: Vec<Vec<String>> = Vec::new();

        let mut inner = Vec::new();
        inner.push(String::from("username"));
        inner.push(String::from("u2sernam"));
        inner.push(String::from("u3sernam"));
        expected.push(inner);

        let mut inner = Vec::new();
        inner.push(String::from("u4sernam"));
        expected.push(inner);

        let parsed = parse_csv_file(&test_filename);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn validate_args_expects_8() {
        let args1 = vec![String::new(); 8];
        let args2 = vec![String::new(); 7];
        let args3 = vec![String::new(); 9];
        let args4 = vec![String::new(); 1];

        let validate1 = validate_args_len(&args1);
        let validate2 = validate_args_len(&args2);
        let validate3 = validate_args_len(&args3);
        let validate4 = validate_args_len(&args4);

        assert_eq!(validate1, true);
        assert_eq!(validate2, false);
        assert_eq!(validate3, false);
        assert_eq!(validate4, false);
    }

    #[test]
    fn correctly_build_config() {
        let args = vec![
            "cmd".to_string(),
            "a1".to_string(),
            "e308eadf8d161c28edbf1076684eb4f7".to_string(),
            "ece459-1231".to_string(),
            "2023-01-27 14:30".to_string(),
            "15".to_string(),
            "csvfile.csv".to_string(),
            "tokenfile.csv".to_string(),
        ];
        let expected_date_time =
            NaiveDateTime::parse_from_str("2023-01-27 14:30", DATE_TIME_FORMAT).unwrap();
        let expected_date_time = expected_date_time.and_local_timezone(Eastern).unwrap();
        let expected_tolerance = Duration::from_secs(900);

        let config = build_config(&args);

        assert_eq!("a1", config.designation);
        assert_eq!("ece459-1231", config.group_name);
        assert_eq!(expected_date_time, config.due_date_time);
        assert_eq!(expected_tolerance, config.tolerance);
        assert_eq!(
            "e308eadf8d161c28edbf1076684eb4f7",
            config.starter_commit_hash
        )
    }

    #[test]
    fn test_calculate_effective_due_date() {
        let due_date_time =
            NaiveDateTime::parse_from_str("2023-01-27 14:30", DATE_TIME_FORMAT).unwrap();
        let due_date_time = due_date_time.and_local_timezone(Eastern).unwrap();
        let tolerance = Duration::from_secs(900);

        let expected_due_date_time =
            NaiveDateTime::parse_from_str("2023-01-27 14:45", DATE_TIME_FORMAT).unwrap();
        let expected_due_date_time = expected_due_date_time.and_local_timezone(Eastern).unwrap();

        let effective_due_date = calculate_effective_due_date(due_date_time, tolerance);

        assert_eq!(expected_due_date_time, effective_due_date);
        let formatted_date = effective_due_date.format("%Y-%m-%d %H:%M %Z").to_string();
        assert!(
            "2023-01-27 14:45 EST".to_string().eq(&formatted_date)
                || "2023-01-27 14:45 EDT".to_string().eq(&formatted_date)
        )
    }

    #[test]
    fn test_get_last_commit() {
        let _ = env_logger::try_init();
        let user_json = fs::read_to_string("test/resources/exampleuser.json")
            .unwrap_or_else(|_| panic!("Unable to read user data"));
        let project_json = fs::read_to_string("test/resources/exampleproject.json")
            .unwrap_or_else(|_| panic!("Unable to read project data"));
        let branch_json = fs::read_to_string("test/resources/examplebranch.json")
            .unwrap_or_else(|_| panic!("Unable to read branch data"));

        let group = String::from("ece459");
        let proj = String::from("a1-username");
        let starter_commit_hash = String::from("79ca81e76a65ff5009596c6e60b99ad0");
        let server = MockServer::start();
        let get_user_mock = server.mock(|when, then| {
            when.method(GET).path("/api/v4/user");
            then.status(200)
                .header("content-type", "application/json")
                .body(user_json);
        });
        let get_proj_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/ece459%2Fa1-username");
            then.status(200)
                .header("content-type", "application/json")
                .body(project_json);
        });

        let get_branch_mock = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/api/v4/projects/4/repository/branches/main"));
            then.status(200)
                .header("content-type", "application/json")
                .body(branch_json);
        });

        let server_url = server.base_url();
        let server_url = server_url.strip_prefix("http://").unwrap();
        let gitlab = Gitlab::new_insecure(server_url, "00").unwrap();
        let last_commit = get_last_commit(&gitlab, &group, &starter_commit_hash, &proj).unwrap();

        // Check that the URL was actually called!
        get_user_mock.assert();
        get_proj_mock.assert();
        get_branch_mock.assert();
        assert_eq!(
            "2023-01-27 03:44 EST".to_string(),
            last_commit.format("%Y-%m-%d %H:%M %Z").to_string()
        );
    }

    #[test]
    fn test_last_commit_is_null_when_same_as_starter_code() {
        let _ = env_logger::try_init();
        let user_json = fs::read_to_string("test/resources/exampleuser.json")
            .unwrap_or_else(|_| panic!("Unable to read user data"));
        let project_json = fs::read_to_string("test/resources/exampleproject.json")
            .unwrap_or_else(|_| panic!("Unable to read project data"));
        let branch_json = fs::read_to_string("test/resources/examplebranch.json")
            .unwrap_or_else(|_| panic!("Unable to read branch data"));

        let group = String::from("ece459");
        let proj = String::from("a1-username");
        let starter_commit_hash = String::from("7b5c3cc8be40ee161ae89a06bba6229da1032a0c");
        let server = MockServer::start();
        let get_user_mock = server.mock(|when, then| {
            when.method(GET).path("/api/v4/user");
            then.status(200)
                .header("content-type", "application/json")
                .body(user_json);
        });
        let get_proj_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/ece459%2Fa1-username");
            then.status(200)
                .header("content-type", "application/json")
                .body(project_json);
        });

        let get_branch_mock = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/api/v4/projects/4/repository/branches/main"));
            then.status(200)
                .header("content-type", "application/json")
                .body(branch_json);
        });

        let server_url = server.base_url();
        let server_url = server_url.strip_prefix("http://").unwrap();
        let gitlab = Gitlab::new_insecure(server_url, "00").unwrap();
        let last_commit = get_last_commit(&gitlab, &group, &starter_commit_hash, &proj);

        // Check that the URL was actually called!
        get_user_mock.assert();
        get_proj_mock.assert();
        get_branch_mock.assert();
        assert_eq!(last_commit.is_none(), true)
    }

    #[test]
    fn test_get_late_days() {
        let _ = env_logger::try_init();
        let user_json = fs::read_to_string("test/resources/exampleuser.json")
            .unwrap_or_else(|_| panic!("Unable to read user data"));
        let project_json = fs::read_to_string("test/resources/exampleproject.json")
            .unwrap_or_else(|_| panic!("Unable to read project data"));
        let branch_json = fs::read_to_string("test/resources/examplebranch.json")
            .unwrap_or_else(|_| panic!("Unable to read branch data"));

        let starter_commit_hash = String::from("79ca81e76a65ff5009596c6e60b99ad0");
        let due_date = NaiveDateTime::parse_from_str("2023-01-27 14:30", DATE_TIME_FORMAT).unwrap();
        let due_date = due_date.and_local_timezone(Eastern).unwrap();
        let default_tolerance = Duration::from_secs(900);

        let config = GitLabConfig {
            designation: "a1".to_string(),
            starter_commit_hash,
            group_name: "ece459".to_string(),
            due_date_time: due_date,
            tolerance: default_tolerance,
        };
        let mut repo_members = Vec::new();
        let mut inner = Vec::new();
        inner.push(String::from("username"));
        repo_members.push(inner);

        let server = MockServer::start();
        let get_user_mock = server.mock(|when, then| {
            when.method(GET).path("/api/v4/user");
            then.status(200)
                .header("content-type", "application/json")
                .body(user_json);
        });
        let get_proj_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/ece459%2Fece459-a1-username");
            then.status(200)
                .header("content-type", "application/json")
                .body(project_json);
        });

        let get_branch_mock = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/api/v4/projects/4/repository/branches/main"));
            then.status(200)
                .header("content-type", "application/json")
                .body(branch_json);
        });

        let server_url = server.base_url();
        let server_url = server_url.strip_prefix("http://").unwrap();
        let gitlab = Gitlab::new_insecure(server_url, "00").unwrap();
        get_late_days(gitlab, repo_members, config);

        // Check that the URL was actually called!
        get_user_mock.assert();
        get_proj_mock.assert();
        get_branch_mock.assert();
        let expected_output_file = "ece459-a1-latedays.csv";
        let expected_nochanges_file = "ece459-a1-nochange.csv";
        let output_contents = fs::read_to_string(expected_output_file)
            .unwrap_or_else(|_| panic!("Unable to read user data"));
        assert_eq!("username,0\n", output_contents);

        remove_file(Path::new(expected_output_file)).unwrap();
        remove_file(Path::new(expected_nochanges_file)).unwrap();
    }

    #[test]
    fn test_get_late_days_group() {
        let _ = env_logger::try_init();
        let user_json = fs::read_to_string("test/resources/exampleuser.json")
            .unwrap_or_else(|_| panic!("Unable to read user data"));
        let project_json = fs::read_to_string("test/resources/exampleproject.json")
            .unwrap_or_else(|_| panic!("Unable to read project data"));
        let branch_json = fs::read_to_string("test/resources/examplebranch.json")
            .unwrap_or_else(|_| panic!("Unable to read branch data"));

        let starter_commit_hash = String::from("79ca81e76a65ff5009596c6e60b99ad0");
        let due_date = NaiveDateTime::parse_from_str("2023-01-27 14:30", DATE_TIME_FORMAT).unwrap();
        let due_date = due_date.and_local_timezone(Eastern).unwrap();
        let default_tolerance = Duration::from_secs(900);

        let config = GitLabConfig {
            designation: "a2".to_string(),
            starter_commit_hash,
            group_name: "ece459".to_string(),
            due_date_time: due_date,
            tolerance: default_tolerance,
        };
        let mut repo_members = Vec::new();
        let mut inner = Vec::new();
        inner.push(String::from("username"));
        inner.push(String::from("u2sernam"));
        repo_members.push(inner);

        let server = MockServer::start();
        let get_user_mock = server.mock(|when, then| {
            when.method(GET).path("/api/v4/user");
            then.status(200)
                .header("content-type", "application/json")
                .body(user_json);
        });
        let get_proj_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/ece459%2Fece459-a2-g1");
            then.status(200)
                .header("content-type", "application/json")
                .body(project_json);
        });

        let get_branch_mock = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/api/v4/projects/4/repository/branches/main"));
            then.status(200)
                .header("content-type", "application/json")
                .body(branch_json);
        });

        let server_url = server.base_url();
        let server_url = server_url.strip_prefix("http://").unwrap();
        let gitlab = Gitlab::new_insecure(server_url, "00").unwrap();
        get_late_days(gitlab, repo_members, config);

        // Check that the URL was actually called!
        get_user_mock.assert();
        get_proj_mock.assert();
        get_branch_mock.assert();
        let expected_output_file = "ece459-a2-latedays.csv";
        let expected_nochanges_file = "ece459-a2-nochange.csv";
        let output_contents = fs::read_to_string(expected_output_file)
            .unwrap_or_else(|_| panic!("Unable to read user data"));
        assert_eq!("username,0\nu2sernam,0\n", output_contents);

        remove_file(Path::new(expected_output_file)).unwrap();
        remove_file(Path::new(expected_nochanges_file)).unwrap();
    }

    #[test]
    fn test_get_late_days_when_no_changes() {
        let _ = env_logger::try_init();
        let user_json = fs::read_to_string("test/resources/exampleuser.json")
            .unwrap_or_else(|_| panic!("Unable to read user data"));
        let project_json = fs::read_to_string("test/resources/exampleproject.json")
            .unwrap_or_else(|_| panic!("Unable to read project data"));
        let branch_json = fs::read_to_string("test/resources/examplebranch.json")
            .unwrap_or_else(|_| panic!("Unable to read branch data"));

        let starter_commit_hash = String::from("7b5c3cc8be40ee161ae89a06bba6229da1032a0c");
        let due_date = NaiveDateTime::parse_from_str("2023-01-27 14:30", DATE_TIME_FORMAT).unwrap();
        let due_date = due_date.and_local_timezone(Eastern).unwrap();
        let default_tolerance = Duration::from_secs(900);

        let config = GitLabConfig {
            designation: "a1".to_string(),
            starter_commit_hash,
            group_name: "ece459".to_string(),
            due_date_time: due_date,
            tolerance: default_tolerance,
        };
        let mut repo_members = Vec::new();
        let mut inner = Vec::new();
        inner.push(String::from("username"));
        repo_members.push(inner);

        let server = MockServer::start();
        let get_user_mock = server.mock(|when, then| {
            when.method(GET).path("/api/v4/user");
            then.status(200)
                .header("content-type", "application/json")
                .body(user_json);
        });
        let get_proj_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/ece459%2Fece459-a1-username");
            then.status(200)
                .header("content-type", "application/json")
                .body(project_json);
        });

        let get_branch_mock = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/api/v4/projects/4/repository/branches/main"));
            then.status(200)
                .header("content-type", "application/json")
                .body(branch_json);
        });

        let server_url = server.base_url();
        let server_url = server_url.strip_prefix("http://").unwrap();
        let gitlab = Gitlab::new_insecure(server_url, "00").unwrap();
        get_late_days(gitlab, repo_members, config);

        // Check that the URL was actually called!
        get_user_mock.assert();
        get_proj_mock.assert();
        get_branch_mock.assert();
        let expected_output_file = "ece459-a1-latedays.csv";
        let expected_nochanges_file = "ece459-a1-nochange.csv";
        let nochanges_content = fs::read_to_string(expected_nochanges_file)
            .unwrap_or_else(|_| panic!("Unable to read user data"));
        assert_eq!("username\n", nochanges_content);

        remove_file(Path::new(expected_output_file)).unwrap();
        remove_file(Path::new(expected_nochanges_file)).unwrap();
    }
}
