//! Offline numeric references generated from official Cfx documentation.

use std::sync::OnceLock;

pub const CONTROLS_SOURCE_URL: &str = "https://docs.fivem.net/docs/game-references/controls/";
pub const PED_CONFIG_FLAGS_SOURCE_URL: &str = "https://docs.fivem.net/natives/?_0x1913FE4CBF41C463";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    pub id: u32,
    pub name: &'static str,
    /// Documented default QWERTY binding; empty means the source did not specify one.
    pub keyboard: &'static str,
    /// Documented default Xbox binding; empty means the source did not specify one.
    pub controller: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PedConfigFlag {
    pub id: u32,
    /// Official reference symbol, including unknown hash placeholders and original spellings.
    pub name: &'static str,
    /// Only actual documented prose, never a behavioral interpretation of the symbol.
    pub description: Option<&'static str>,
}

fn control_table() -> &'static [Control] {
    static TABLE: OnceLock<Vec<Control>> = OnceLock::new();
    TABLE.get_or_init(|| {
        include_str!("../data/controls.tsv")
            .lines()
            .map(|line| {
                let mut fields = line.split('\t');
                Control {
                    id: fields.next().unwrap().parse().expect("generated control ID"),
                    name: fields.next().expect("generated control name"),
                    keyboard: fields.next().expect("generated keyboard binding"),
                    controller: fields.next().expect("generated controller binding"),
                }
            })
            .collect()
    })
}

fn flag_table() -> &'static [PedConfigFlag] {
    static TABLE: OnceLock<Vec<PedConfigFlag>> = OnceLock::new();
    TABLE.get_or_init(|| {
        include_str!("../data/ped_config_flags.tsv")
            .lines()
            .map(|line| {
                let mut fields = line.split('\t');
                PedConfigFlag {
                    id: fields.next().unwrap().parse().expect("generated ped flag ID"),
                    name: fields.next().expect("generated ped flag name"),
                    description: fields.next().filter(|description| !description.is_empty()),
                }
            })
            .collect()
    })
}

pub fn control(id: u32) -> Option<Control> {
    let table = control_table();
    table.binary_search_by_key(&id, |row| row.id).ok().map(|index| table[index])
}

pub fn controls() -> impl Iterator<Item = Control> {
    control_table().iter().copied()
}

pub fn ped_config_flag(id: u32) -> Option<PedConfigFlag> {
    let table = flag_table();
    table.binary_search_by_key(&id, |row| row.id).ok().map(|index| table[index])
}

pub fn ped_config_flags() -> impl Iterator<Item = PedConfigFlag> {
    flag_table().iter().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_defaults_and_absent_bindings_are_preserved() {
        let context = control(51).unwrap();
        assert_eq!(context.name, "INPUT_CONTEXT");
        assert_eq!((context.keyboard, context.controller), ("E", "DPAD RIGHT"));
        assert_eq!(control(3).unwrap().keyboard, "(NONE)");
        assert_eq!(control(360).unwrap().keyboard, "");
        assert_eq!(control(39).unwrap().keyboard, "[");
        assert_eq!(control(243).unwrap().keyboard, "~ / `");
        assert!(control(u32::MAX).is_none());
        assert!(controls().count() >= 361);
    }

    #[test]
    fn flag_symbols_are_not_invented_descriptions() {
        let flag = ped_config_flag(32).unwrap();
        assert_eq!(flag.name, "CPED_CONFIG_FLAG_WillFlyThroughWindscreen");
        assert_eq!(flag.description, None);
        assert_eq!(ped_config_flag(54).unwrap().name, "CPED_CONFIG_FLAG_DissableAutoFallOffTests");
        assert_eq!(ped_config_flag(463).unwrap().name, "_0x1AA79A25");
        assert!(ped_config_flag(u32::MAX).is_none());
        assert!(ped_config_flags().count() >= 464);
    }

    #[test]
    fn all_generated_rows_are_sorted_complete_and_round_trip() {
        for (expected, row) in controls().enumerate() {
            assert_eq!(row.id as usize, expected);
            assert_eq!(control(row.id), Some(row));
            assert!(!row.name.contains(['<', '>', '\\', '\t']));
            assert!(!row.keyboard.contains(['<', '>', '\t']));
            assert!(!row.controller.contains(['<', '>', '\t']));
        }
        for (expected, row) in ped_config_flags().enumerate() {
            assert_eq!(row.id as usize, expected);
            assert_eq!(ped_config_flag(row.id), Some(row));
            assert!(row.description.is_none(), "the current official enum has no behavioral prose");
        }
        assert!(include_str!("../data/controls.tsv").lines().all(|line| line.split('\t').count() == 4));
        assert!(include_str!("../data/ped_config_flags.tsv").lines().all(|line| line.split('\t').count() == 3));
    }
}
