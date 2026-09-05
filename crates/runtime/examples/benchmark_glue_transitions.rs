//! Measures complete offline Glue transitions and following Vulkan present intervals.

use std::cell::RefCell;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Error as IoError, ErrorKind, Write};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use solarity_asset::{ArchiveCatalog, AssetStore, LightCatalog};
use solarity_cpu::BlizzardRand;
use solarity_runtime::{
    ClientApplication, GlueBenchmarkAction, GlueBenchmarkResult, GlueBenchmarkScreen,
    GlueBenchmarkStep, RuntimeConfiguration,
};
use solarity_ui::{
    UiCharacterCreationState, UiCharacterDirectory, UiCharacterEquipment, UiCharacterInfo,
    UiCharacterPetPreview,
};

fn main() -> Result<(), Box<dyn Error>> {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .from_env()?;
    let _subscriber = tracing_subscriber::fmt().with_env_filter(filter).try_init();
    let mut args = std::env::args_os().skip(1);
    let following_frames = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?
        .parse::<NonZeroUsize>()?;
    let output = PathBuf::from(args.next().ok_or_else(usage_error)?);
    let configuration = RuntimeConfiguration::from_arguments(args)?;
    let steps = scenario(&configuration)?;
    let mut application = ClientApplication::start(configuration)?;
    println!(
        "adapter={} extent={:?}",
        application.vulkan_report().device_name(),
        application.vulkan_report().extent()
    );
    let capture_directory = std::env::var_os("SOLARITY_GLUE_CAPTURE_DIR").map(PathBuf::from);
    if let Some(directory) = &capture_directory {
        for (index, step) in steps.iter().enumerate() {
            println!(
                "capture step={} path={}",
                step.name,
                directory.join(format!("{index:02}.ppm")).display()
            );
        }
    }
    let result = application.benchmark_glue_steps(
        &steps,
        following_frames,
        Duration::from_secs(30),
        capture_directory.as_deref(),
    );
    let shutdown = application.shutdown();
    let samples = result?;
    shutdown?;
    let mut writer = BufWriter::new(File::create(output)?);
    writeln!(writer, "step,phase,frame,duration_ms,action_ms,ready_ms")?;
    for sample in samples {
        write_samples(&mut writer, &sample)?;
        print_summary(&sample);
    }
    writer.flush()?;
    Ok(())
}

/// Builds button identities from the same DBC order that stock Glue enumerates.
fn scenario(
    configuration: &RuntimeConfiguration,
) -> Result<Vec<GlueBenchmarkStep>, Box<dyn Error>> {
    let archives =
        ArchiveCatalog::discover(configuration.data_root().clone(), configuration.locale())?;
    let mut store = AssetStore::mount(archives)?;
    let ghost_colors = LightCatalog::load(&mut store)?.model_light_colors(3, 0)?;
    println!(
        "ghost_light parameter=3 half_minutes=0 ambient={:?} diffuse={:?}",
        ghost_colors.ambient(),
        ghost_colors.diffuse()
    );
    let creation = UiCharacterCreationState::load(
        &mut store,
        false,
        Rc::new(RefCell::new(BlizzardRand::new(0))),
    )?;
    let races = creation.available_races();
    let classes = creation.available_classes();
    let mut steps = vec![
        step(
            "login",
            GlueBenchmarkAction::Screen(GlueBenchmarkScreen::Login),
            GlueBenchmarkScreen::Login,
        ),
        step(
            "selection-human-cold",
            GlueBenchmarkAction::Sequence(vec![
                GlueBenchmarkAction::Screen(GlueBenchmarkScreen::CharacterSelection),
                GlueBenchmarkAction::Directory(selection_directory()),
            ]),
            GlueBenchmarkScreen::CharacterSelection,
        ),
    ];
    for (label, index) in [
        ("nightelf-cold", 2),
        ("bloodelf-cold", 3),
        ("nightelf-warm", 2),
        ("ghost-cold", 4),
        ("nightelf-after-ghost", 2),
        ("ghost-warm", 4),
        ("human-warm", 1),
    ] {
        steps.push(click(
            &format!("selection-{label}"),
            &format!("CharSelectCharacterButton{index}"),
            GlueBenchmarkScreen::CharacterSelection,
        ));
    }
    steps.push(step(
        "creation-enter",
        GlueBenchmarkAction::Screen(GlueBenchmarkScreen::CharacterCreation),
        GlueBenchmarkScreen::CharacterCreation,
    ));
    steps.push(click(
        "creation-human",
        &button(&races, "Human", "Race")?,
        GlueBenchmarkScreen::CharacterCreation,
    ));
    steps.push(click(
        "creation-male",
        "CharacterCreateGenderButtonMale",
        GlueBenchmarkScreen::CharacterCreation,
    ));
    steps.push(click(
        "creation-warrior",
        &button(&classes, "WARRIOR", "Class")?,
        GlueBenchmarkScreen::CharacterCreation,
    ));
    for (label, class) in [
        ("paladin", "PALADIN"),
        ("priest", "PRIEST"),
        ("warrior-return", "WARRIOR"),
    ] {
        steps.push(click(
            &format!("creation-{label}"),
            &button(&classes, class, "Class")?,
            GlueBenchmarkScreen::CharacterCreation,
        ));
    }
    for axis in 1..=5 {
        for direction in ["Right", "Left"] {
            steps.push(click(
                &format!("customization-{axis}-{direction}"),
                &format!("CharacterCustomizationButtonFrame{axis}{direction}Button"),
                GlueBenchmarkScreen::CharacterCreation,
            ));
        }
    }
    for (label, race) in [
        ("nightelf", "NightElf"),
        ("bloodelf", "BloodElf"),
        ("nightelf-return", "NightElf"),
        ("human-return", "Human"),
    ] {
        steps.push(click(
            &format!("creation-{label}"),
            &button(&races, race, "Race")?,
            GlueBenchmarkScreen::CharacterCreation,
        ));
    }
    steps.push(click(
        "creation-randomize",
        "CharCreateRandomizeButton",
        GlueBenchmarkScreen::CharacterCreation,
    ));
    Ok(steps)
}

/// Uses valid enum-time appearances already covered by the representation validator.
fn selection_directory() -> UiCharacterDirectory {
    let entries = [
        (
            1,
            "Human",
            "Human",
            1,
            [0; 5],
            UiCharacterPetPreview::default(),
            0,
        ),
        (
            4,
            "Night Elf",
            "NightElf",
            3,
            [0, 1, 6, 5, 2],
            UiCharacterPetPreview::new(2711, 80, 25),
            0,
        ),
        (
            10,
            "Blood Elf",
            "BloodElf",
            2,
            [0; 5],
            UiCharacterPetPreview::default(),
            0,
        ),
        (
            4,
            "Night Elf",
            "NightElf",
            3,
            [0, 1, 6, 5, 2],
            UiCharacterPetPreview::new(2711, 80, 25),
            0x2000,
        ),
    ];
    UiCharacterDirectory::new(
        entries
            .into_iter()
            .enumerate()
            .map(
                |(index, (race, name, file, class, appearance, pet, flags))| {
                    UiCharacterInfo::new(
                        index as u64 + 1,
                        format!("Benchmark{file}"),
                        name.to_owned(),
                        race,
                        file.to_owned(),
                        match class {
                            1 => "Warrior",
                            2 => "Paladin",
                            _ => "Hunter",
                        }
                        .to_owned(),
                        class,
                        80,
                        None,
                        2,
                        0,
                        appearance,
                        if class == 3 {
                            hunter_equipment()
                        } else {
                            [UiCharacterEquipment::default(); 23]
                        },
                        pet,
                        flags,
                        0,
                    )
                },
            )
            .collect(),
        "Human".to_owned(),
    )
}

/// Reuses the authored equipment case admitted by the installed-data validator.
fn hunter_equipment() -> [UiCharacterEquipment; 23] {
    [
        UiCharacterEquipment::new(65_131, 1, 0),
        UiCharacterEquipment::new(64_190, 2, 0),
        UiCharacterEquipment::new(64_829, 3, 0),
        UiCharacterEquipment::new(7_904, 4, 0),
        UiCharacterEquipment::new(64_840, 5, 0),
        UiCharacterEquipment::new(65_035, 6, 0),
        UiCharacterEquipment::new(64_832, 7, 0),
        UiCharacterEquipment::new(64_822, 8, 0),
        UiCharacterEquipment::new(64_421, 9, 0),
        UiCharacterEquipment::new(64_827, 10, 0),
        UiCharacterEquipment::new(64_225, 11, 0),
        UiCharacterEquipment::new(64_230, 11, 0),
        UiCharacterEquipment::new(68_106, 12, 0),
        UiCharacterEquipment::new(68_109, 12, 0),
        UiCharacterEquipment::new(28_951, 16, 0),
        UiCharacterEquipment::new(64_554, 17, 0),
        UiCharacterEquipment::default(),
        UiCharacterEquipment::new(64_356, 15, 0),
        UiCharacterEquipment::new(20_621, 19, 0),
        UiCharacterEquipment::new(56_653, 18, 0),
        UiCharacterEquipment::default(),
        UiCharacterEquipment::default(),
        UiCharacterEquipment::default(),
    ]
}

/// Resolves the physical class or faction-grouped race order into a stock button name.
fn button(values: &[(String, String, bool)], token: &str, kind: &str) -> Result<String, IoError> {
    let index = values
        .iter()
        .position(|(_, file, _)| file.eq_ignore_ascii_case(token))
        .ok_or_else(|| {
            IoError::new(
                ErrorKind::InvalidData,
                format!("missing benchmark {kind} {token}"),
            )
        })?;
    Ok(format!("CharacterCreate{kind}Button{}", index + 1))
}

/// Couples one action to the route whose complete frame proves readiness.
fn step(
    name: &str,
    action: GlueBenchmarkAction,
    expected_screen: GlueBenchmarkScreen,
) -> GlueBenchmarkStep {
    GlueBenchmarkStep {
        name: name.to_owned(),
        action,
        expected_screen,
    }
}

/// Creates an ordinary pointer stimulus for one exact live GlueXML button.
fn click(name: &str, button: &str, screen: GlueBenchmarkScreen) -> GlueBenchmarkStep {
    step(name, GlueBenchmarkAction::Click(button.to_owned()), screen)
}

/// Writes every raw interval so long publication and following frames remain reviewable.
fn write_samples(writer: &mut impl Write, sample: &GlueBenchmarkResult) -> Result<(), IoError> {
    for (phase, frames) in [
        ("transition", &sample.transition_frames),
        ("following", &sample.following_frames),
    ] {
        for (index, duration) in frames.iter().enumerate() {
            writeln!(
                writer,
                "{},{phase},{index},{:.6},{:.6},{:.6}",
                sample.name,
                duration.as_secs_f64() * 1000.0,
                sample.action_duration.as_secs_f64() * 1000.0,
                sample.ready_duration.as_secs_f64() * 1000.0
            )?;
        }
    }
    Ok(())
}

/// Reports transition maxima separately from following-frame average and tail latency.
fn print_summary(sample: &GlueBenchmarkResult) {
    let mut frames = sample.following_frames.clone();
    frames.sort_unstable();
    let total = frames.iter().map(Duration::as_secs_f64).sum::<f64>();
    let p99 = frames
        .get((frames.len() * 99 / 100).min(frames.len().saturating_sub(1)))
        .copied()
        .unwrap_or_default();
    println!(
        "{} action_ms={:.3} ready_ms={:.3} transition_max_ms={:.3} following_fps={:.1} following_p99_ms={:.3} following_max_ms={:.3}",
        sample.name,
        sample.action_duration.as_secs_f64() * 1000.0,
        sample.ready_duration.as_secs_f64() * 1000.0,
        sample
            .transition_frames
            .iter()
            .max()
            .copied()
            .unwrap_or_default()
            .as_secs_f64()
            * 1000.0,
        frames.len() as f64 / total,
        p99.as_secs_f64() * 1000.0,
        frames.last().copied().unwrap_or_default().as_secs_f64() * 1000.0
    );
}

fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        format!(
            "usage: benchmark_glue_transitions <following frames per step> <output.csv> {}",
            RuntimeConfiguration::usage()
        ),
    )
}
