# Repo Late Day Calculator

Here's a small tool that I wrote to help me make a CSV file showing late day usage in the course. Late days are reported in units of full days, so 2 hours late counts as 1, 22 hours late counts as 1, 47 hours counts as 2, etc.

Expectations:

1. Student repos are created in a gitlab group (e.g., `ece459-1231`) and the names follow the format in my other project to create repos: [create-project-repos](github.com/jzarnett/create-project-repos)
2. Your file of students and groups to create is prepared (correctly); see the "Usage" section for what to do.

Output:

The tool creates a csv file in the format `id, late days used` (e.g., `jzarnett,0`). One line per student, whether it's a single student project or multi-student group.

The CSV file is created without headers since your import routine probably wants something annoying to autogenerate. LEARN, why.

## Usage
I tried to make it easy but there are a few things that could not be avoided. It takes six commandline arguments (order and format matter, sadly).

Formally:
```
executable <designation> <gitlab_group_name> <due_date_time> <tolerance_in_mins> <list_of_student_groups.csv> <token_file>
```

In order then:
### `designation`
The designation refers to how this repos you want to evalute are designated: typically assignment 1 would be given as `a1`, but you could say it's the final exam by putting `final`, or a project by `p`. Whatever you did with the create-repos tool!

### `gitlab_group_name`
This is the group in gitlab where the repos to check are found. So if the course and term I'm running this in are ECE 459 and 1231 (Winter 2023), I would choose `ece459-1231`.

### `due_date_time`
This is the due date and time for the assignment or project, in the format `"%Y-%m-%d %H:%M"` (e.g., `2023-01-24 20:36`. This is the time you officially tell students the deliverable is due. The program is going to assume you mean Canadian Eastern (Standard|Daylight) time depending on the local time when you run it. 

### `tolerance_in_mins`
The tolerance in minutes; ie how late does a submission have to be to count as actually late? We recognize that life isn't always neat and tidy, so we may be generous and not charge the student a full late day if they are submitting only a few minutes late. The effective due date is calculated using the provided `due_date_time` above plus the tolerance. So if the input due date is `2023-01-24 21:00` and the tolerance is `30` then the effective due date is calculated as `2023-01-24 21:30`. Could I have skipped this and just made you manually add the tolerance to the due date? Yes. But you're welcome.

### `list_of_students_or_groups.csv`
Provide the filename of a CSV (comma-separated-value) format file that contains information about the students and groups that exist. The only content here is student usernames (e.g., jzarnett for me). You have two options about what to do here (and can mix and match):

If there is EXACTLY one username on a line, the repo will be expected to be `group-desigation-username`, so `ece459-1231-a1-jzarnett`.

If there are MULTIPLE usernames on a line, the repo will be expected to be `group-designation-g{linenumber+1}`. So if this is the 8th line of the CSV file it will be created as `ece459-1231-proj-g9`.

Why is it like this and not using the usernames of the users who are members of the project? Because this way you can reuse the same input file you gave to the repo creation tool with no changes. 

### `token_file`
A plain text file containing your gitlab user token. You need to have the necessary permissions to access all the repos in question. No newline or anything at the end of the file.


## TODOs
- This isn't parallelized, though in practice I'd like to try doing 2-3 repos at once. Helps when there's 400+ students.
- At some point the default branch should change from `master` to `main` and it would be nice if the program could handle that automatically
- Letting you give params in any order might be good.
- Maybe I should revisit the decision to use the csv with student names and should instead look at membership in the group. And maybe get all repos in the group and just filter out the ones that don't match the pattern. That would eliminate the CSV entirely.
- When I get out of programmer prison for not writing tests, I'm sure I will have to write unit tests as part of my restitution.

## Changelog

### 1.0.1
Tests and cargo clippy

### 1.0.0
Initial, non-parallelized version
