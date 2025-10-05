use clap::Parser;
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::Path;

use anki_bridge::prelude::*;

const ID_DELIMETER: &str = "#id:";

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Deck to use
    #[arg(short, long, default_value = "sync-default")]
    deck: String,

    /// File to sync
    #[arg()]
    file: String,
}

#[derive(PartialEq, Debug, Clone)]
struct AnkiNote {
    side_a: String,
    side_b: String,
    id: i64,
}

fn read_file_to_string(path: &Path) -> String {
    let mut file = File::open(path).expect("Failed to open file");
    let mut f_string = String::new();
    file.read_to_string(&mut f_string)
        .expect("File reading went wrong");
    return f_string;
}

fn main() {
    let args = Args::parse();
    let path = Path::new(&args.file);
    let file_string = read_file_to_string(path);

    let mut input_lines: Vec<String> = file_string.lines().map(|l| l.to_string()).collect();

    let mut lines_iter = input_lines.iter();
    let mut line_count = 0;

    let mut next: &str;

    let mut note_db: Vec<AnkiNote> = Vec::new();

    let deck_name = &args.deck;

    // try and get all the notes
    loop {
        // next line
        match lines_iter.next() {
            Some(l) => next = l,
            None => break,
        }

        if !next.contains("::") {
            continue;
        }

        line_count += 1;
        let note_id: i64;

        // already has an id
        if next.contains(ID_DELIMETER) {
            // mame id
            let line_split: Vec<&str> = next.split(ID_DELIMETER).collect();
            note_id = parse_id(line_split.get(1).unwrap_or(&"0"));
            if note_id == 0 {
                eprintln!("Bad id at {}.", line_count);
                continue;
            }
            next = line_split.get(0).unwrap();
        } else {
            note_id = 0;
        }

        let word_pair: Vec<&str> = next.split("::").collect();

        match word_pair.len() {
            0..=1 => {
                eprintln!("No delimeters at line {}.", line_count);
                continue;
            }
            2 => (),
            _ => {
                eprintln!("Too many delimeters at line {}.", line_count);
                continue;
            }
        }

        let word_a = word_pair.get(0).unwrap().to_string();
        let word_b = word_pair.get(1).unwrap().to_string();

        if word_a.is_empty() || word_b.is_empty() {
            eprintln!("Empty pair at line {}.", line_count);
            continue;
        }

        // id uz existuje
        if note_db.iter().any(|c| c.id == note_id && note_id != 0) {
            eprintln!("Duplicate id at line {}.", line_count);
            continue;
        }

        note_db.push(AnkiNote {
            side_a: word_a,
            side_b: word_b,
            id: note_id,
        });
    }

    let anki = AnkiClient::default();

    let all_note_ids_dist = anki_get_notes(&anki, deck_name);
    let all_notes_dist = anki_get_notes_info(&anki, &all_note_ids_dist);

    dbg!(&note_db);
    dbg!(&all_note_ids_dist);

    let mut new_notes: Vec<AnkiNote> = vec![];
    let mut old_notes: Vec<AnkiNote> = vec![];

    for note_info in all_notes_dist {
        for n in note_db.iter_mut() {
            if n.side_a.trim() != note_info.fields["Front"].value.trim()
                || n.side_b.trim() != note_info.fields["Back"].value.trim()
            {
                if n.id != note_info.note_id {
                    continue;
                } else {
                    // note exists, try to update
                    println!("Updating note: {}.", note_info.note_id);
                    anki_update_note(&anki, &n);
                    dbg!("+1");
                    if old_notes.contains(&n) {
                        panic!("Duplicates in old_notes!");
                    }
                    old_notes.push(n.clone());
                }
            } else {
                if n.id == 0 {
                    println!("Restoring note id {} back", note_info.note_id);
                    n.id = note_info.note_id;
                }

                if !old_notes.contains(&n) {
                    old_notes.push(n.clone());
                }
            }
        }
        //let current_note = &old_notes[i];
    }

    //for (i, note) in old_notes_dist.iter().enumerate() {}

    dbg!(&new_notes);
    dbg!(&old_notes);

    let mut new_notes_ids = anki_add_notes(&anki, &new_notes, deck_name);

    // append id numbers
    for note in new_notes.iter_mut() {
        // id uz mame
        let new_id = new_notes_ids.pop().expect("Failed to assign ids to notes.");
        replace_id(&mut input_lines, note, new_id);
    }

    let file = OpenOptions::new()
        .write(true)
        .open(&args.file)
        .expect("Failed to open the file.");
    let mut writer = BufWriter::new(file);
    input_lines
        .iter()
        .for_each(|l| writeln!(writer, "{}", l).expect("Failed to write a line the file."));

    println!("Done.");
}

fn anki_add_notes(anki: &AnkiClient, notes: &Vec<AnkiNote>, name: &String) -> Vec<i64> {
    let mut note_list: Vec<AddNoteEntry> = vec![];
    for note in notes {
        note_list.push(AddNoteEntry {
            deck_name: name.clone(),
            model_name: "Basic (and reversed card)".to_string(),
            fields: HashMap::from([
                ("Front".to_string(), note.side_a.trim().to_string()),
                ("Back".to_string(), note.side_b.trim().to_string()),
            ]),
            options: AddNoteOptions {
                allow_duplicate: false,
                duplicate_scope: AddNoteDuplicateScope::Deck,
                duplicate_scope_options: AddNoteDuplicateScopeOptions {
                    deck_name: None,
                    check_children: false,
                    check_all_models: false,
                },
            },
            tags: [].to_vec(),
            audio: [].to_vec(),
            video: [].to_vec(),
            picture: [].to_vec(),
        });
    }

    let request = AddNotesRequest { notes: note_list };

    return anki.request(request).expect("Something went wrong.");
}

fn anki_get_notes_info(anki: &AnkiClient, notes_list: &Vec<i64>) -> Vec<NotesInfoResponse> {
    let request = NotesInfoRequest {
        notes: notes_list.clone(),
    };
    return anki.request(request).expect("Something went wrong.");
}

fn anki_get_notes(anki: &AnkiClient, deck: &str) -> Vec<i64> {
    let mut request_str: String = "deck:".to_string();
    request_str.push_str(deck);
    return anki
        .request(FindNotesRequest { query: request_str })
        .expect("Something went wrong. Error: {}")
        .0;
}

fn anki_update_note(anki: &AnkiClient, note: &AnkiNote) {
    return anki
        .request(UpdateNoteFieldsRequest {
            note: UpdateNoteFieldsEntry {
                id: note.id,
                fields: HashMap::from([
                    ("Front".to_string(), note.side_a.clone()),
                    ("Back".to_string(), note.side_b.clone()),
                ]),
                audio: vec![],
                video: vec![],
                picture: vec![],
            },
        })
        .expect("Something went wrong when updating notes.");
}

fn parse_id(string_id: &str) -> i64 {
    match string_id.parse::<i64>() {
        Ok(n) => return n,
        Err(_) => return 0,
    }
}

fn replace_id(file_lines: &mut Vec<String>, note: &AnkiNote, new_id: i64) {
    let line_n: usize;
    match file_lines
        .iter()
        .position(|x| x.contains(&note.id.to_string()))
    {
        Some(x) => line_n = x,
        None => {
            eprintln!("Couldn't find the correct id in the file.");
            return;
        }
    }
    let replace = file_lines[line_n]
        .split(&("  ".to_owned() + ID_DELIMETER))
        .nth(0)
        .unwrap()
        .to_string()
        + &("  ".to_string() + ID_DELIMETER + &new_id.to_string());

    println!("Line: {}, changed to: {}", line_n, replace);
    file_lines[line_n] = replace;
}

fn append_id(file_lines: &mut Vec<String>, note: &AnkiNote) {
    let find_string: String = note.side_a.to_string() + "::" + &note.side_b;
    let line_n: usize;
    match &file_lines.iter().position(|x| x.contains(&find_string)) {
        Some(n) => line_n = *n,
        None => {
            eprintln!("Couldn't find the pair in the file.");
            return;
        }
    }
    file_lines[line_n] += &("  ".to_string() + ID_DELIMETER + &note.id.to_string());
}
