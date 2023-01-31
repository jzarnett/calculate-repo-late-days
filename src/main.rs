use std::fs::File;
use std::io::{BufRead, BufReader, Lines, Write};
use std::time::Duration;
use std::{env, fs};

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use chrono_tz::Canada::Eastern;
use chrono_tz::Tz;
use gitlab::api::projects::repository::branches::BranchBuilder;
use gitlab::api::{projects, Query};
use gitlab::Gitlab;
use serde::Deserialize;

const UW_GITLAB_URL: &str = "git.uwaterloo.ca";
// One day this will be "main" but for now...
const DEFAULT_BRANCH_NAME: &str = "master";
const DATE_TIME_FORMAT: &str = "%Y-%m-%d %H:%M";
const MINS_PER_DAY: f64 = 60.0 * 24.0;

#[derive(Debug, Deserialize)]
struct Project {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct Commit {
    committed_date: DateTime<FixedOffset>,
}

#[derive(Debug, Deserialize)]
struct Branch {
    default: bool,
    commit: Commit,
}

struct GitLabConfig {
    designation: String,
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
    if args.len() != 7 {
        println!(
            "Usage: {} <designation> <gitlab_group_name> <due_date_time> <tolerance_in_mins> <list_of_student_groups.csv> <token_file>",
            args.get(0).unwrap()
        );
        println!(
            "Example: {} a1 ece459-1231 \"2023-01-27 23:59\" 60 students.csv token.git",
            args.get(0).unwrap()
        );
        return false;
    }
    true
}

fn build_config(args: &[String]) -> GitLabConfig {
    let duration_minutes: u64 = args.get(4).unwrap().parse().unwrap();
    let naive_date_time =
        NaiveDateTime::parse_from_str(args.get(3).unwrap(), DATE_TIME_FORMAT).unwrap();
    let due_date = naive_date_time.and_local_timezone(Eastern).unwrap();

    let config = GitLabConfig {
        designation: String::from(args.get(1).unwrap()),
        group_name: String::from(args.get(2).unwrap()),
        due_date_time: due_date,
        tolerance: Duration::from_secs(60 * duration_minutes),
    };
    config
}

fn get_late_days(client: Gitlab, repo_members: Vec<Vec<String>>, config: GitLabConfig) {
    let output_file_name = format! {"{}-{}-latedays.csv", config.group_name, config.designation};
    let mut output_file = File::create(output_file_name).unwrap();
    let effective_due_date = calculate_effective_due_date(&config);

    for i in 0..repo_members.len() {
        let group_or_student = repo_members.get(i).unwrap();
        let project_name = if group_or_student.len() == 1 {
            format!(
                "{}-{}-{}",
                config.group_name,
                config.designation,
                group_or_student.get(0).unwrap()
            )
        } else {
            format!("{}-{}-g{}", config.group_name, config.designation, (i + 1))
        };

        let last_commit = get_last_commit(&client, &config.group_name, &project_name);
        let lateness_in_days = calculate_lateness(last_commit, effective_due_date);
        for student in group_or_student {
            let file_line = format!("{student},{lateness_in_days}\n");
            output_file.write_all(file_line.as_bytes()).unwrap();
        }
    }
}

fn calculate_effective_due_date(config: &GitLabConfig) -> DateTime<Tz> {
    config
        .due_date_time
        .checked_add_signed(chrono::Duration::from_std(config.tolerance).unwrap())
        .unwrap()
}

fn calculate_lateness(last_commit: DateTime<Tz>, due_date_time: DateTime<Tz>) -> i64 {
    println!("Last commit was on {last_commit}; due date was {due_date_time}");
    if last_commit.le(&due_date_time) {
        return 0;
    }
    let diff = (last_commit - due_date_time).num_minutes();
    println!("This is is {diff} minutes late");
    1 + (diff as f64 / MINS_PER_DAY).floor() as i64
}

fn get_last_commit(client: &Gitlab, group_name: &String, project_name: &String) -> DateTime<Tz> {
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
    branch.commit.committed_date.with_timezone(&Eastern)
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
    use chrono::NaiveDateTime;
    use chrono_tz::Canada::Eastern;
    use std::fs::{remove_file, File};
    use std::io::Write;
    use std::path::Path;

    use crate::{calculate_lateness, parse_csv_file, read_token_file, DATE_TIME_FORMAT};

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
        let test_filename = String::from("tests/resources/simple.csv");
        let mut expected: Vec<Vec<String>> = Vec::new();
        let mut inner = Vec::new();
        inner.push(String::from("username"));
        expected.push(inner);

        let parsed = parse_csv_file(&test_filename);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn can_parse_group_csv() {
        let test_filename = String::from("tests/resources/group.csv");
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
        let test_filename = String::from("tests/resources/group_spaces.csv");
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
        let test_filename = String::from("tests/resources/multiple.csv");
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
        let test_filename = String::from("tests/resources/newline_eof.csv");
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
        let test_filename = String::from("tests/resources/group_uneven_sizes.csv");
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
        let test_filename = String::from("tests/resources/mixed.csv");
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
}
