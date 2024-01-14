extern crate rust_embed;
extern crate clap;
extern crate rand;
extern crate atty;
extern crate libc;
extern crate base64;
extern crate crossterm;
extern crate console;
extern crate png;

use std::io::{self, Read, Write};
use std::fs::File;
use rust_embed::RustEmbed;
use clap::Parser;
use rand::seq::SliceRandom;
use base64::{engine::general_purpose, Engine};
use libc::{ftruncate, mmap, munmap, shm_open, close, memcpy};
use libc::{MAP_SHARED, O_RDWR, O_CREAT, PROT_WRITE, S_IRUSR, S_IWUSR};
use libc::{c_char, c_void, off_t, size_t};
use std::{ptr, str};
use crossterm::cursor::{MoveRight, MoveToNextLine, MoveToPreviousLine};
use crossterm::execute;
use console::{Term, Key};
use std::{thread, time};
use std::sync::mpsc;

const STORAGE_ID: *const c_char = b"/RCOWSAY_IMAGE\0".as_ptr() as *const c_char;
const STORAGE_ID_STR: &str = "/RCOWSAY_IMAGE";
const TEST_ID: *const c_char = b"/RCOWSAY_TEST\0".as_ptr() as *const c_char;
const TEST_ID_STR: &str = "/RCOWSAY_TEST";

#[derive(RustEmbed)]
#[folder = "src/cows/"]
struct Asset;

#[derive(Parser)]
#[command(author, version, about, long_about = "Cowsay generates an ASCII picture of a cow saying something provided by the user.\nIf run with no arguments, it accepts standard input, word-wraps the message given\nat about 40 columns, and prints the cow saying the given message on standard output.")]
struct Args {
    /// Which cow picture to use
    #[arg(short = 'f')]
    cow: Option<String>,

    /// Max. width of cow text bubble
    #[arg(short = 'W')]
    width: Option<usize>,

    /// Disable word wrap
    #[arg(short = 'n')]
    nowrap: bool,

    /// Invokes borg mode
    #[arg(short = 'b')]
    borg: bool,

    /// Causes the cow to appear dead
    #[arg(short = 'd')]
    dead: bool,

    /// Invokes greedy mode
    #[arg(short = 'g')]
    greedy: bool,

    /// Causes a state of paranoia to come over the cow
    #[arg(short = 'p')]
    paranoid: bool,

    /// Makes the cow appear thoroughly stoned
    #[arg(short = 's')]
    stoned: bool,

    /// Yields a tired cow
    #[arg(short = 't')]
    tired: bool,

    /// Is somewhat the opposite of -t, and initiates wired mode
    #[arg(short = 'w')]
    wired: bool,

    /// Brings on the cow’s youthful appearance
    #[arg(short = 'y')]
    youthful: bool,

    /// Selects the appearance of the cow’s eyes, in which case the first two characters of the argument string EYE_STRING will be used
    #[arg(short = 'e', value_name = "EYE_STRING")]
    eyes: Option<String>,

    /// Selects the appearance of the cow’s tongue, in which case the first character of the argument string TONGUE_STRING will be used
    #[arg(short = 'T', value_name = "TONGUE_STRING")]
    tongue: Option<String>,

    /// Lists the available cow pictures
    #[arg(short = 'l')]
    list: bool,

    /// Chooses a random cow picture
    #[arg(short = 'r')]
    random: bool,

    /// Modern cowsay, with unicode and images (kitty only)
    #[arg(short = 'm')]
    modern: bool,

    /// Alternative modern cowsay, with unicode and animations (kitty only, work in progress)
    #[arg(short = 'c')]
    pipboy: bool,

    #[arg()]
    message: Vec<String>,
}

struct CowBubble {
    sleft: &'static str,
    sright: &'static str,
    topleft: &'static str,
    midleft: &'static str,
    botleft: &'static str,
    topright: &'static str,
    midright: &'static str,
    botright: &'static str,
    topc: &'static str,
    bottomc: &'static str,
}

struct Offset {
    x: u32,
    y: i32,
}

fn list_cows() -> Vec<String> {
    Asset::iter()
        .filter(|x| x.split("/").last().unwrap().ends_with(".cow"))
        .map(|x| x.split("/").last().unwrap().replace(".cow", ""))
        .collect::<Vec<String>>()
}

fn unmap_shared_memory(addr: *mut c_void, length: size_t) {
    unsafe {
        munmap(addr, length);
    }
}

fn create_shared_memory(storage_id: *const c_char, data: &[u8]) -> *mut c_void {
    // Load data into shared memory segment
    
    // Create shared memory mapping
    let addr: *mut c_void = unsafe {
        let null = ptr::null_mut();
        //let fd   = shm_open(storage_id, O_RDWR | O_CREAT, (S_IRUSR | S_IWUSR) as size_t);
        let fd   = shm_open(storage_id, O_RDWR | O_CREAT, (S_IRUSR | S_IWUSR) as u32);
        let _res = ftruncate(fd, data.len() as off_t);
        let addr = mmap(null, data.len() as size_t, PROT_WRITE, MAP_SHARED, fd, 0);
        close(fd);
        
        addr
    };

    // Copy data to shared memory
    let pdata = data.as_ptr() as *const c_void;
    unsafe {
        memcpy(addr, pdata, data.len());
    }

    addr
}

fn offset_cursor(offset: &Offset) {
    if offset.y > 0 {
        execute!(std::io::stdout(), MoveToNextLine(offset.y as u16)).expect("Error adjusting cursor position"); 
    } else {
        execute!(std::io::stdout(), MoveToPreviousLine(-offset.y as u16)).expect("Error adjusting cursor position");
    }
    execute!(std::io::stdout(), MoveRight(offset.x as u16)).expect("Error adjusting cursor position"); 
}

fn is_kitty() -> bool {
    if let Ok(term) = std::env::var("TERM") {
        if term.contains("kitty") {
            return true;
        }
    }
    false
}

fn kitty_local_support() -> bool {
    // Create temporary shared memory "image"
    let image: [u8; 3] = [128, 49, 167];
    let addr = create_shared_memory(TEST_ID, &image);

    // Send query to terminal
    print!("\x1b_Gi=31,s=1,v=1,a=q,t=s,f=24;{}\x1b\\", general_purpose::STANDARD.encode(TEST_ID_STR));
    std::io::stdout().flush().expect("Error sending control sequence to terminal");

    // Read response from terminal
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let term = Term::stdout();
        let mut response = String::from("");

        while let Ok(key) = term.read_key() {
            if let Key::Char(c) = key {
                response.push(c);
            }
            if key == Key::UnknownEscSeq(vec!['\\']) || key == Key::Unknown {
                break;
            }
        }
        tx.send(response).unwrap();
    });
    let response = rx.recv_timeout(time::Duration::from_millis(100)).unwrap_or(String::from(""));
    unmap_shared_memory(addr, image.len() as size_t);

    if response == "Gi=31;OK" {
        return true;
    }
    
    false
}

fn kitty_image_local(offset: &Offset, image: &[u8]) {
    // Create shared memory object from image
    let addr = create_shared_memory(STORAGE_ID, image);

    offset_cursor(offset);

    print!("\x1b_Gf=100,a=T,t=s;{}\x1b\\", general_purpose::STANDARD.encode(STORAGE_ID_STR));
    std::io::stdout().flush().expect("Error sending control sequence to terminal");
    println!("");
    
    // Unmap the shared memory
    unmap_shared_memory(addr, image.len() as size_t);
}

fn kitty_image_remote(offset: &Offset, mut image: &[u8]) {
    offset_cursor(offset);

    // Convert PNG data to raw rgba / rgb data
    let mut decoder = png::Decoder::new(image.by_ref());
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().expect("Error reading PNG data");
    let mut img_data: Vec<u8> = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut img_data).expect("Error reading PNG image");
    let format = if info.color_type == png::ColorType::Rgba { 32 } else { 24 };
    let height = info.height;
    let width = info.width;

    //let img = image::load_from_memory(image).expect("Error loading image data");
    //let rgba = img.to_rgba8();
    //let img_data = rgba.as_raw();
    //let width = img.width();
    //let height = img.height();
    //let format = 32;

    let encoded = general_purpose::STANDARD.encode(img_data);
    let mut m = if encoded.len() > 4096 { 1 } else { 0 };

    let mut iter = encoded.chars().peekable();
    let first_chunk: String = iter.by_ref().take(4096).collect();

    print!("\x1b_Gf={},s={},v={},a=T,t=d,m={};{}\x1b\\", format, width, height, m, first_chunk);

    while iter.peek().is_some() {
        let chunk: String = iter.by_ref().take(4096).collect();
        m = if chunk.len() == 4096 { 1 } else { 0 };
        print!("\x1b_Gm={};{}\x1b\\", m, chunk);
    }

    std::io::stdout().flush().expect("Error sending control sequence to terminal");
    println!("");
}

fn cow_image(offset: &Offset, image: &[u8]) {
    // Check if kitty runs locally or remote
    if kitty_local_support() {
        kitty_image_local(offset, image);
    } else {
        kitty_image_remote(offset, image);
    }
}

fn format_animal(s: String, thoughts: &str, eyes: &str, tongue: &str) -> String {
    s.split("\n")
        .filter(|&x| !x.starts_with("##") && !x.contains("EOC"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .replace("$eyes", eyes)
        .replace("$thoughts", thoughts)
        .replace("$tongue", tongue)
        .replace("\\\\", "\\")
        .replace("\\@", "@")
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

fn get_index(s: &str, max_chars: usize) -> Option<usize> {
    match s.char_indices().nth(max_chars) {
        None => None,
        Some((idx, _)) => Some(idx),
    }
}

fn make_bubble(s: String, width: usize, thinking: bool, wrap: bool, modern: bool) -> String {
    let mut result: Vec<String> = Vec::new();
    let pad = " ";
    
    let cowb = if modern {
        CowBubble {
            sleft: if thinking {"┊"} else {"│"},
            sright: if thinking {"┊"} else {"│"},
            topleft: if thinking {"╭"} else {"┌"},
            midleft: if thinking {"┊"} else {"│"},
            botleft: if thinking {"╰"} else {"└"},
            topright: if thinking {"╮"} else {"┐"},
            midright: if thinking {"┊"} else {"│"},
            botright: if thinking {"╯"} else {"┘"},
            topc: if thinking {"┄"} else {"─"},
            bottomc: if thinking {"┄"} else {"─"},
        }
    } else {
        CowBubble {
            sleft: if thinking {"("} else {"<"},
            sright: if thinking {")"} else {">"},
            topleft: if thinking {"("} else {"/"},
            midleft: if thinking {"("} else {"|"},
            botleft: if thinking {"("} else {"\\"},
            topright: if thinking {")"} else {"\\"},
            midright: if thinking {")"} else {"|"},
            botright: if thinking {")"} else {"/"},
            topc: "_",
            bottomc: "-",
        }
    };

    // Linewrap

    let slice: Vec<String> = s.lines().map(|tmp| str::to_string(tmp).replace("\t", "    ")).collect();
    for line in slice {
        if line.chars().count() <= width {
            // The entire line fits without issues
            result.push(line);
            continue;
        }

        // Line too long, needs to be split
        if !wrap {
            // Don't line wrap, so just truncate and continue
            result.push(truncate(&line, width - 3).to_owned() + "...");
            continue;
        }

        // Split line by space characters and add words until width is filled
        let subslice: Vec<&str> = line.as_str().split(' ').collect();
        let mut res = String::new();
        let mut buffer: &str;
        let mut buffer2: &str;
        let mut i = 0;
        let mut splitpoint;
        let mut maxlength;

        // this bit is not unicode-save (subslice[i])
        while i < subslice.len() {
            buffer = subslice[i];
            if res.len() + buffer.len() < width {
                // Word fits into line, so add it and move to next word
                if res.len() > 0 { res.push_str(" "); }
                res.push_str(buffer);
                i += 1;
                continue;
            }
            
            // Word doesn't fit into line, it has to be split potentially

            // There is already a substancial amount of words in the buffer, so just end this
            // line and end this loop iteration and try again with the same word
            if res.chars().count() >= (width as f32 * 0.75) as usize {
                result.push(res);
                res = String::new();
                continue;
            }

            // Split long word
            loop {
                // Get available space on current line
                maxlength = width - res.chars().count() - 1;

                // the remaining buffer is too long for the line
                if buffer.chars().count() > maxlength {
                    // split the buffer and add the first half and end the line
                    splitpoint = get_index(buffer, maxlength - 1).expect("Error: String can't be split!");
                    (buffer2, buffer) = buffer.split_at(splitpoint);
                    
                    if res.len() > 0 { res.push_str(" "); }
                    res.push_str(buffer2);
                    res.push_str("-");
                    result.push(res);
                    res = String::new();
                    continue;
                }
                
                // the remaining buffer fits into line
                if res.len() > 0 { res.push_str(" "); }
                res.push_str(buffer);
                result.push(res);
                res = String::new();
                i += 1;
                break;
            }
            
        }
        // Last line may not have been added yet
        if res.len() > 0 { result.push(res); }
    } 

    // Bookend lines with bubble chars
    let mut longest = 0;
    let reslen = result.len() - 1;
    for (index, line) in result.iter_mut().enumerate() {
        match index {
            0 => match reslen {
                0 | 1 => *line = vec![cowb.sleft, line, cowb.sright].join(" "),
                _ => { *line = vec![if modern {cowb.midleft} else {cowb.topleft}, line, if modern {cowb.midright} else {cowb.topright}].join(" ") }
            },
            x if x < reslen => *line = vec![cowb.midleft, line, cowb.midright].join(" "),
            y if y == reslen => match reslen {
                1 => *line = vec![cowb.sleft, line, cowb.sright].join(" "),
                _ => *line = vec![if modern {cowb.midleft} else {cowb.botleft}, line, if modern {cowb.midright} else {cowb.botright}].join(" ")
            },
            _ => panic!("Unable to create text bubble"),
        }
        if line.chars().count() > longest {
            longest = line.chars().count();
        }
    }

    // Pad to longest line
    for line in &mut result {
        let padding = longest - line.chars().count();  //line.len();
        let lastchar = line.char_indices().map(|(i, _)| i).last().unwrap();
        line.insert_str(lastchar, pad.repeat(padding).as_str());
        //line.insert_str(line.len() - 1, pad.repeat(padding).as_str());
    }

    let top_bottom = longest - 2;
    let mut top = String::new();
    let mut bottom = String::new();
    if modern {
        top = vec![cowb.topleft, cowb.topc.repeat(top_bottom).as_str(), cowb.topright].join("");
        bottom = vec![cowb.botleft, cowb.bottomc.repeat(top_bottom).as_str(), cowb.botright].join("");
    } else {
        top.push_str(" ");
        bottom.push_str(" ");
        top.push_str(cowb.topc.repeat(top_bottom).as_str());
        bottom.push_str(cowb.bottomc.repeat(top_bottom).as_str());
    }
    result.insert(0, top);
    result.push(bottom);

    result.join("\n")
}

fn main() {

    // Get command line parameters
    let args = Args::parse();

    if args.list {
        let list = list_cows();
        println!("{:?}", list);
        return;
    }

    // Modern cowsay - only kitty supported at the moment
    let modern = if (args.modern || args.pipboy) && is_kitty() {
        true
    } else {
        false
    };

    let mut cow = args.cow.unwrap_or("default".to_owned());
    if args.random {
        let cows = list_cows();
        cow = cows.choose(&mut rand::thread_rng()).unwrap().to_owned();
    }

    let width = args.width.unwrap_or(40);

    let mut message = args.message.join(" ");
    if message == "" && atty::isnt(atty::Stream::Stdin) {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).expect("Reading from stdin failed");
        message = buffer.trim_end().to_string();
    }
    if message == "" { message = "I've got nothing to say".to_owned() }

    let tongue = args.tongue.as_ref().map_or(" ", |s| &s[..1] );

    let eyes = [(args.borg, "=="),
                (args.dead, "xx"),
                (args.greedy, "$$"),
                (args.paranoid, "@@"),
                (args.stoned, "**"),
                (args.tired, "--"),
                (args.wired, "OO"),
                (args.youthful, ".."),
                (args.eyes.is_some(), args.eyes.as_ref().map(|s| &s[..2]).unwrap_or("")),
                (true, "oo")]
                    .iter()
                    .filter(|&x| x.0)
                    .collect::<Vec<_>>()[0].1;

    let voice;
    let think;
    if std::env::current_exe().ok().unwrap().ends_with("cowthink") {
        think = true;
        voice = "o";
    } else {
        think = false;
        voice = if modern {"╲"} else {"\\"};
    }


    let mut cowbody = String::new();

    // Print cow text bubble
    let bubble = make_bubble(message, width, think, !args.nowrap, modern);
    println!("{}", bubble);

    if modern {
        println!("        {}", voice);
        println!("         {}", voice);

        // Load png to memory
        let mut cowimage: Vec<u8> = Vec::new();
        match cow.contains(".png") {
            true => {
                let mut f = File::open(&cow).unwrap();
                f.read_to_end(&mut cowimage).expect("Error reading image file!");
            }
            false => {
                let cowpng = if args.pipboy { "pipboy.png" } else { "cow.png" };
                cowimage = Asset::get(&cowpng).unwrap().data.to_vec();
            }
        }

        let offset = if args.pipboy {
            Offset {
                x: 4,
                y: -2
            }
        } else {
            Offset {
                x: 11,
                y: -1
            }
        };

        // Print modern, graphical cow
        cow_image(&offset, cowimage.as_slice());
    } else {
        match cow.contains(".cow") {
            true => {
                let mut f = File::open(&cow).unwrap();
                f.read_to_string(&mut cowbody).expect(&format!("Couldn't read cowfile {}", cow));
            }
            false => {
                let fmt = &format!("{}.cow", &cow);
                cowbody = std::str::from_utf8(Asset::get(&fmt).unwrap().data.as_ref()).unwrap().to_string();
            }
        }
        
        // Print text-based cow body
        println!("{}", format_animal(cowbody, voice, eyes, tongue));
    }
}
