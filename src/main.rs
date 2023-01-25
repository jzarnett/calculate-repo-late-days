use std::fs::File;
use std::io::Write;
use std::time::Duration;
use std::{env, fs};

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use chrono_tz::Canada::Eastern;
use chrono_tz::Tz;
use csv::ReaderBuilder;
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
    if args.len() != 7 {
        println!(
            "Usage: {} <designation> <gitlab_group_name> <due_date_time> <tolerance_in_mins> <list_of_student_groups.csv> <token_file>",
            args.get(0).unwrap()
        );
        println!(
            "Example: {} a1 ece459-1231 \"2023-01-27 23:59\" 60 students.csv token.git",
            args.get(0).unwrap()
        );
        return;
    }
    let token = read_token_file(args.get(6).unwrap());
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

    let repo_members = parse_csv_file(args.get(5).unwrap());
    let client = Gitlab::new(String::from(UW_GITLAB_URL), token).unwrap();

    get_late_days(client, repo_members, config)
}

fn get_late_days(client: Gitlab, repo_members: Vec<Vec<String>>, config: GitLabConfig) {
    let output_file_name = format! {"{}-{}-latedays.csv", config.group_name, config.designation};
    let mut output_file = File::create(output_file_name).unwrap();
    let effective_due_date = config
        .due_date_time
        .checked_add_signed(chrono::Duration::from_std(config.tolerance).unwrap())
        .unwrap();

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
            let file_line = format!("{},{}\n", student, lateness_in_days);
            output_file.write_all(file_line.as_bytes()).unwrap();
        }
    }
}

fn calculate_lateness(last_commit: DateTime<Tz>, due_date_time: DateTime<Tz>) -> i64 {
    println!(
        "Last commit was on {}; due date was {}",
        last_commit, due_date_time
    );
    if last_commit.lt(&due_date_time) {
        return 0;
    }
    let diff = last_commit - due_date_time;
    println!("This is is {} minutes late", diff.num_minutes());
    1 + (diff.num_minutes() as f64 / MINS_PER_DAY).floor() as i64
}

fn get_last_commit(client: &Gitlab, group_name: &String, project_name: &String) -> DateTime<Tz> {
    let project_builder = projects::ProjectBuilder::default()
        .project(format!("{}/{}", group_name, project_name))
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
            "Project {} uses a different default branch than expected {}!",
            project_name, DEFAULT_BRANCH_NAME
        )
    }
    branch.commit.committed_date.with_timezone(&Eastern)
}

fn parse_csv_file(filename: &String) -> Vec<Vec<String>> {
    let mut result: Vec<Vec<String>> = Vec::new();
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .from_path(filename)
        .unwrap();

    for line in rdr.records() {
        let line = line.unwrap();
        let mut inner = Vec::new();
        for user in line.iter() {
            inner.push(String::from(user))
        }
        result.push(inner);
    }
    result
}

fn read_token_file(filename: &String) -> String {
    fs::read_to_string(filename)
        .unwrap_or_else(|_| panic!("Unable to read token from file {}", filename))
}
