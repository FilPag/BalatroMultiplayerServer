use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameMode {
    #[serde(rename = "gamemode_mp_attrition")]
    Attrition,
    #[serde(rename = "gamemode_mp_showdown")]
    Showdown,
    #[serde(rename = "gamemode_mp_survival")]
    Survival,
    #[serde(rename = "gamemode_mp_coopSurvival")]
    CoopSurvival,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyOptions {
    pub back: &'static str,
    pub challenge: i32,
    pub custom_seed: &'static str,
    pub death_on_round_loss: bool,
    pub different_decks: bool,
    pub different_seeds: bool,
    pub disable_live_and_timer_hud: bool,
    pub gamemode: GameMode,
    pub gold_on_life_loss: bool,
    pub multiplayer_jokers: bool,
    pub no_gold_on_round_loss: bool,
    pub normal_bosses: bool,
    pub pvp_start_round: i32,
    pub ruleset: &'static str,
    pub showdown_starting_antes: i32,
    pub sleeve: &'static str,
    pub stake: i32,
    pub starting_lives: i32,
    pub timer_base_seconds: i32,
    pub timer_increment_seconds: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindChoice {
    pub small: Option<String>,
    pub big: Option<String>,
    pub boss: Option<String>,
}

impl BlindChoice {
    pub fn empty() -> Self {
        Self {
            small: None,
            big: None,
            boss: None,
        }
    }

    pub fn pvp_only() -> Self {
        Self {
            small: Some("bl_pvp".to_string()),
            big: Some("bl_pvp".to_string()),
            boss: Some("bl_pvp".to_string()),
        }
    }

    pub fn boss_only(boss: &str) -> Self {
        Self {
            small: None,
            big: None,
            boss: Some(boss.to_string()),
        }
    }
}

pub struct GameModeData {
    pub default_options: LobbyOptions,
    pub get_blind_from_ante: fn(ante: i32, options: &LobbyOptions) -> BlindChoice,
}

impl GameMode {
    pub fn get_data(&self) -> &'static GameModeData {
        match self {
            GameMode::Attrition => &ATTRITION_DATA,
            GameMode::Showdown => &SHOWDOWN_DATA,
            GameMode::Survival => &SURVIVAL_DATA,
            GameMode::CoopSurvival => &COOP_SURVIVAL_DATA,
        }
    }

    pub fn get_default_options(&self) -> LobbyOptions {
        self.get_data().default_options.clone()
    }

    pub fn get_blind_from_ante(&self, ante: i32, options: &LobbyOptions) -> BlindChoice {
        (self.get_data().get_blind_from_ante)(ante, options)
    }
}

// Attrition game mode
static ATTRITION_DATA: GameModeData = GameModeData {
    default_options: LobbyOptions {
        back: "Red Deck",
        challenge: 0,
        custom_seed: "random",
        death_on_round_loss: false,
        different_decks: false,
        different_seeds: false,
        disable_live_and_timer_hud: false,
        gamemode: GameMode::Attrition,
        gold_on_life_loss: true,
        multiplayer_jokers: true,
        no_gold_on_round_loss: false,
        normal_bosses: false,
        pvp_start_round: 2,
        ruleset: "ruleset_mp_standard",
        showdown_starting_antes: 3,
        sleeve: "sleeve_casl_none",
        stake: 1,
        starting_lives: 4,
        timer_base_seconds: 150,
        timer_increment_seconds: 60,
    },
    get_blind_from_ante: |_ante, _options| BlindChoice::boss_only("bl_pvp"),
};

// Showdown game mode
static SHOWDOWN_DATA: GameModeData = GameModeData {
    default_options: LobbyOptions {
        back: "Red Deck",
        challenge: 0,
        custom_seed: "random",
        death_on_round_loss: false,
        different_decks: false,
        different_seeds: false,
        disable_live_and_timer_hud: false,
        gamemode: GameMode::Showdown,
        gold_on_life_loss: true,
        multiplayer_jokers: true,
        no_gold_on_round_loss: false,
        normal_bosses: false,
        pvp_start_round: 2,
        ruleset: "ruleset_mp_standard",
        showdown_starting_antes: 3,
        sleeve: "sleeve_casl_none",
        stake: 1,
        starting_lives: 4,
        timer_base_seconds: 150,
        timer_increment_seconds: 60,
    },
    get_blind_from_ante: |ante, options| {
        let starting_antes = options.showdown_starting_antes;
        if ante <= starting_antes {
            BlindChoice::empty()
        } else {
            BlindChoice::pvp_only()
        }
    },
};

// Survival game mode
static SURVIVAL_DATA: GameModeData = GameModeData {
    default_options: LobbyOptions {
        back: "Red Deck",
        challenge: 0,
        custom_seed: "random",
        death_on_round_loss: false,
        different_decks: false,
        different_seeds: false,
        disable_live_and_timer_hud: false,
        gamemode: GameMode::Survival,
        gold_on_life_loss: true,
        multiplayer_jokers: true,
        no_gold_on_round_loss: false,
        normal_bosses: false,
        pvp_start_round: 20,
        ruleset: "ruleset_mp_standard",
        showdown_starting_antes: 3,
        sleeve: "sleeve_casl_none",
        stake: 1,
        starting_lives: 4,
        timer_base_seconds: 150,
        timer_increment_seconds: 60,
    },
    get_blind_from_ante: |_ante, _options| BlindChoice::empty(),
};

// CoopSurvival game mode
static COOP_SURVIVAL_DATA: GameModeData = GameModeData {
    default_options: LobbyOptions {
        back: "Red Deck",
        challenge: 0,
        custom_seed: "random",
        death_on_round_loss: true,
        different_decks: true,
        different_seeds: true,
        disable_live_and_timer_hud: false,
        gamemode: GameMode::CoopSurvival,
        ruleset: "ruleset_mp_coop",
        gold_on_life_loss: false,
        multiplayer_jokers: false,
        no_gold_on_round_loss: true,
        normal_bosses: true,
        pvp_start_round: 2,
        showdown_starting_antes: 3,
        sleeve: "sleeve_casl_none",
        stake: 1,
        starting_lives: 2,
        timer_base_seconds: 150,
        timer_increment_seconds: 60,
    },
    get_blind_from_ante: |_ante, _options| BlindChoice::empty(),
};

impl std::str::FromStr for GameMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Attrition" => Ok(GameMode::Attrition),
            "Showdown" => Ok(GameMode::Showdown),
            "Survival" => Ok(GameMode::Survival),
            "CoopSurvival" => Ok(GameMode::CoopSurvival),
            _ => Err(format!("Unknown game mode: {}", s)),
        }
    }
}

impl std::fmt::Display for GameMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameMode::Attrition => write!(f, "Attrition"),
            GameMode::Showdown => write!(f, "Showdown"),
            GameMode::Survival => write!(f, "Survival"),
            GameMode::CoopSurvival => write!(f, "CoopSurvival"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_showdown_blind_logic() {
        let options = GameMode::Showdown.get_default_options();

        // Before starting antes - no blinds
        let blind = GameMode::Showdown.get_blind_from_ante(1, &options);
        assert!(blind.small.is_none());
        assert!(blind.big.is_none());
        assert!(blind.boss.is_none());

        // After starting antes - PvP blinds
        let blind = GameMode::Showdown.get_blind_from_ante(4, &options);
        assert_eq!(blind.small, Some("bl_pvp".to_string()));
        assert_eq!(blind.big, Some("bl_pvp".to_string()));
        assert_eq!(blind.boss, Some("bl_pvp".to_string()));
    }

    #[test]
    fn test_attrition_blind_logic() {
        let options = GameMode::Attrition.get_default_options();
        let blind = GameMode::Attrition.get_blind_from_ante(1, &options);
        assert_eq!(blind.boss, Some("bl_pvp".to_string()));
    }
}
