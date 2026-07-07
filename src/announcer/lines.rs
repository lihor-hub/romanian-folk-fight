//! Static announcer line pools: pure data, no logic. Adding or editing lines
//! never touches the announcer systems — extend the [`pool`] arms (and, for a
//! brand-new hook, [`LineKey`] plus [`LineKey::ALL`]).
//!
//! Placeholders understood by `super::fill_placeholders`:
//! `{attacker}`/`{actor}`/`{winner}` name the acting fighter,
//! `{defender}`/`{loser}`/`{opponent}` name the other one, and
//! `{dmg}`/`{amount}` carry the event's number.

/// Which pool an announcement draws from: one per [`CombatEvent`] variant
/// plus the fight-start and boss-intro hooks (the roster issue feeds the
/// latter through [`super::AnnouncementRequest`]).
///
/// [`CombatEvent`]: crate::combat::CombatEvent
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKey {
    /// Both fighters enter the arena.
    FightStart,
    /// A boss steps in; `{opponent}` is the boss.
    BossIntro,
    /// A strike missed.
    Missed,
    /// A normal hit for `{dmg}`.
    Hit,
    /// A critical hit for `{dmg}`.
    Crit,
    /// The defender's guard halved the hit to `{dmg}`.
    Blocked,
    /// The actor raised a guard.
    Guarded,
    /// The actor rested for `{amount}` stamina.
    Rested,
    /// A strike was rejected for lack of stamina.
    OutOfStamina,
    /// `{loser}` fell; `{winner}` stands.
    Defeated,
    /// `{winner}` beat the lap-1 final boss `{loser}`: the run is won.
    Victory,
}

impl LineKey {
    /// Every key, for coverage tests and per-key bookkeeping.
    pub const ALL: [LineKey; 11] = [
        LineKey::FightStart,
        LineKey::BossIntro,
        LineKey::Missed,
        LineKey::Hit,
        LineKey::Crit,
        LineKey::Blocked,
        LineKey::Guarded,
        LineKey::Rested,
        LineKey::OutOfStamina,
        LineKey::Defeated,
        LineKey::Victory,
    ];

    /// Number of keys; sizes the announcer's last-pick table.
    pub const COUNT: usize = Self::ALL.len();

    /// Stable dense index into per-key tables.
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// The line pool for one key. Every pool holds at least five lines; the
/// coverage test in `super::tests` enforces it.
pub const fn pool(key: LineKey) -> &'static [&'static str] {
    match key {
        LineKey::FightStart => &[
            "Doamnelor și domnilor: {actor} contra {opponent}! Lăsați sarmalele, începe!",
            "S-a deschis hora bătăii: {actor} și {opponent} intră în arenă!",
            "{actor} contra {opponent}! Câștigătorul ia gloria, învinsul spală vasele.",
            "Liniște în sat! {actor} și {opponent} își încearcă norocul în arenă.",
            "Băgați de seamă: {actor} contra {opponent}. Pauza de țuică se amână!",
        ],
        LineKey::BossIntro => &[
            "Tremurați, dragilor: {opponent} a coborât din munți!",
            "Se zice că {opponent} mănâncă zmei la micul dejun.",
            "{opponent} intră în arenă. Babele își fac cruce, cocoșii tac.",
            "Ascundeți cașcavalul: a venit {opponent}!",
            "Nici Sfarmă-Piatră nu s-a pus cu {opponent}. Baftă, viteazule!",
        ],
        LineKey::Missed => &[
            "{attacker} taie aerul. Aerul rămâne neînvins.",
            "{attacker} lovește ca prin ceață. Ceața nici nu clipește.",
            "Pe lângă! {attacker} ar nimeri mai ușor un țânțar cu furca.",
            "{attacker} ratează de parcă ar ținti cu ochii închiși la horă.",
            "Vântul zice mersi: {attacker} l-a pieptănat frumos.",
        ],
        LineKey::Hit => &[
            "{attacker} lovește zdravăn: {dmg} daune, ca la carte!",
            "Poc! {attacker} împarte {dmg} daune cum se împart colacii la nuntă.",
            "{attacker} dă cu sete: {dmg} daune. S-a auzit până-n sat!",
            "Lovitură cinstită de la {attacker}: {dmg} daune, fără șpagă.",
            "{attacker} lovește cum bate bunica covorul: {dmg} daune!",
        ],
        LineKey::Crit => &[
            "{attacker} lovește ca Sfarmă-Piatră! {dmg} daune!",
            "CRITIC! {attacker} a dat cu toată dragostea: {dmg} daune!",
            "Mamă, mamă! {attacker} despică cerul: {dmg} daune!",
            "{attacker} lovește de sar scânteile ca la fierărie: {dmg} daune!",
            "Așa lovitură n-a mai văzut nici moș Ion: {attacker} face {dmg} daune!",
        ],
        LineKey::Blocked => &[
            "{defender} se apără ca o cetate din Carpați! Doar {dmg} daune trec.",
            "{defender} parează! Doar {dmg} daune se strecoară peste gard.",
            "Scut de nădejde: {defender} oprește aproape tot. {dmg} daune, atât.",
            "{defender} stă ca poarta maramureșeană: doar {dmg} daune printre stâlpi.",
            "Degeaba bate {attacker}: {defender} e zid de mănăstire. {dmg} daune.",
        ],
        LineKey::Guarded => &[
            "{actor} ridică garda cum ridică gospodarul gardul: temeinic.",
            "{actor} se închide ca cetatea Neamțului la asediu.",
            "{actor} își ține scutul cum ține bunica broboada: strâns.",
            "{actor} se apără, prudent ca țăranul la târg.",
            "Garda sus! {actor} nu mai primește musafiri astăzi.",
        ],
        LineKey::Rested => &[
            "{actor} își trage sufletul și se gândește la sarmale.",
            "{actor} respiră adânc: +{amount} stamina și poftă de viață.",
            "Pauză de mămăligă: {actor} recuperează {amount} stamina.",
            "{actor} se odihnește ca după coasă: +{amount} stamina.",
            "{actor} visează o clipă la cozonac și recuperează {amount} stamina.",
        ],
        LineKey::OutOfStamina => &[
            "{actor} vrea, dar picioarele zic ba. N-are stamina!",
            "{actor} e stors ca un burete în zi de curățenie.",
            "Fără stamina, {actor} amenință doar cu privirea.",
            "{actor} suflă greu ca moara fără vânt. Poate la anul!",
            "{actor} ridică brațul... și îl pune la loc. N-are vlagă.",
        ],
        LineKey::Defeated => &[
            "S-a terminat! {loser} pleacă acasă pe jos, prin pădure.",
            "{winner} câștigă! {loser} va povesti la cârciumă altă versiune.",
            "Gata hora: {loser} s-a culcat înainte de finală.",
            "{loser} cade ca frunza toamna. {winner} rămâne în picioare!",
            "Praf și pulbere: {loser} își caută demnitatea prin arenă.",
        ],
        LineKey::Victory => &[
            "{winner} a doborât {loser}! Legenda se scrie chiar acum!",
            "Sunați clopotele: {winner} e noul viteaz al viteazilor!",
            "{winner} a răpus spaima voinicilor! Sarmale pentru tot satul!",
            "Nici în basme nu s-a mai pomenit: {winner} a curățat toată arena!",
            "Hora victoriei! {winner} rămâne în picioare, {loser} intră în folclor.",
        ],
    }
}
