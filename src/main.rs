use anyhow::Context;
use bmp::Image;
use config_file::FromConfigFile;
use directories::UserDirs;
use rdev::{listen, Event, EventType};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Deserialize)]
pub struct Config {
    width: u32,
    height: u32,
    offset: u32,
}

const PIXEL_SIZE: u32 = 256;

fn refresh_destop() {
    use windows::Win32::UI::Shell::SHChangeNotify;
    use windows::Win32::UI::Shell::SHCNE_ASSOCCHANGED;
    use windows::Win32::UI::Shell::SHCNF_IDLIST;

    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

fn main() -> anyhow::Result<()> {
    let config = Config::from_config_file("config.toml").context("Failed to load config")?;

    let desktop = get_desktop_dir()?;

    clear_old_files(&desktop)?;

    let mut black = Image::new(PIXEL_SIZE, PIXEL_SIZE);
    for (x, y) in black.coordinates() {
        black.set_pixel(x, y, bmp::Pixel::new(0, 0, 0));
    }
    let mut red = Image::new(PIXEL_SIZE, PIXEL_SIZE);
    for (x, y) in red.coordinates() {
        red.set_pixel(x, y, bmp::Pixel::new(255, 0, 0));
    }

    for o in 0..config.offset {
        black.save(desktop.join(format!("ds_o{}.bmp", o)))?;
    }

    for y in 0..config.height {
        for x in 0..config.width {
            black.save(Path::new(&desktop).join(format!("ds_p{}-{}.bmp", y, x)))?;
        }
    }

    let mut snake_bits = vec![(1, 1)];

    #[derive(Copy, Clone)]
    enum SnakeDir {
        Up,
        Down,
        Left,
        Right,
    }

    let snake_dir = Arc::new(Mutex::new(SnakeDir::Right));

    let mut updates = Vec::new();

    let mut food_pos = (2, 1);

    fn wrap(val: i32, max: i32) -> i32 {
        if val < 0 {
            max - 1
        } else if val >= max {
            0
        } else {
            val
        }
    }

    impl TryFrom<rdev::Key> for SnakeDir {
        type Error = ();
        fn try_from(key: rdev::Key) -> Result<Self, Self::Error> {
            match key {
                rdev::Key::UpArrow => Ok(SnakeDir::Up),
                rdev::Key::DownArrow => Ok(SnakeDir::Down),
                rdev::Key::LeftArrow => Ok(SnakeDir::Left),
                rdev::Key::RightArrow => Ok(SnakeDir::Right),
                _ => Err(()),
            }
        }
    }

    let snake_dir_2 = snake_dir.clone();
    let callback = move |event: Event| {
        if let EventType::KeyPress(k) = event.event_type {
            println!("Key: {:?}", k);
            let new_dir = match *snake_dir_2.lock().unwrap() {
                SnakeDir::Up | SnakeDir::Down => match k.try_into() {
                    Ok(x @ (SnakeDir::Left | SnakeDir::Right)) => Some(x),
                    _ => None,
                },
                SnakeDir::Left | SnakeDir::Right => match k.try_into() {
                    Ok(x @ (SnakeDir::Up | SnakeDir::Down)) => Some(x),
                    _ => None,
                },
            };
            if let Some(new_dir) = new_dir {
                *snake_dir_2.lock().unwrap() = new_dir;
            }
        }
    };

    std::thread::spawn(move || {
        if let Err(error) = listen(callback) {
            println!("Error: {:?}", error)
        }
    });

    loop {
        let (head_x, head_y) = *snake_bits.last().unwrap();
        let head_x = head_x as i32;
        let head_y = head_y as i32;

        let (new_x, new_y) = match *snake_dir.lock().unwrap() {
            SnakeDir::Up => (head_x, head_y - 1),
            SnakeDir::Down => (head_x, head_y + 1),
            SnakeDir::Left => (head_x - 1, head_y),
            SnakeDir::Right => (head_x + 1, head_y),
        };

        let new_x = wrap(new_x, config.width as i32);
        let new_y = wrap(new_y, config.height as i32);

        let snake_new_bit = (new_x as usize, new_y as usize);

        snake_bits.push(snake_new_bit);
        updates.push((snake_new_bit.0, snake_new_bit.1, true));

        if snake_new_bit != food_pos {
            let (tail_x, tail_y) = snake_bits.remove(0);
            updates.push((tail_x, tail_y, false));
        } else {
            food_pos = (
                rand::random::<usize>() % config.width as usize,
                rand::random::<usize>() % config.height as usize,
            );
            updates.push((food_pos.0, food_pos.1, true));
        }

        for (x, y, val) in updates.iter().copied() {
            let img = if val { &red } else { &black };
            img.save(Path::new(&desktop).join(format!("ds_p{}-{}.bmp", y, x)))?;
        }

        updates.clear();

        // refresh desktop
        // yeah, doesn't work well
        //refresh_destop();

        // wait 1 second
        // can't really speed that part up
        std::thread::sleep(std::time::Duration::from_millis(1200));
    }
}

fn clear_old_files(desktop: &PathBuf) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(desktop)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("ds_")
        {
            std::fs::remove_file(&path)?;
        }
    }

    Ok(())
}

fn get_desktop_dir() -> anyhow::Result<PathBuf> {
    let dirs = UserDirs::new().context("Failed to get user directories")?;
    let desktop = dirs
        .desktop_dir()
        .context("Failed to get desktop directory")?
        .join("snake");
    Ok(desktop)
}
