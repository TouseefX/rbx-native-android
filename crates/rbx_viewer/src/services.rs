//! Roblox Studio's Explorer, as data: the order it lists services in and
//! which of Roblox's own services its default view hides. Shared by
//! `rbxstudio`'s Explorer and the browser build's (see `web`).

/// The services Studio's default Explorer view shows, in the order it lists
/// them in, neither alphabetical nor the order the file stores them in.
pub const SERVICE_ORDER: [&str; 14] = [
    "Workspace",
    "Players",
    "Lighting",
    "MaterialService",
    "ReplicatedFirst",
    "ReplicatedStorage",
    "ServerScriptService",
    "ServerStorage",
    "StarterGui",
    "StarterPack",
    "StarterPlayer",
    "Teams",
    "SoundService",
    "TextChatService",
];

/// The order Studio lists root services in, default-visible or not: every
/// service class with an `ExplorerOrder` in Studio's own
/// `ReflectionMetadata.xml` (as mirrored by MaximumADHD's
/// Roblox-Client-Tracker), sorted by it, ties alphabetical the way Studio
/// breaks them. [`SERVICE_ORDER`] is this list minus the hidden ones, which
/// is why a service ticked to show (`Chat`, say) lands between `SoundService`
/// and `TextChatService` instead of after them. Anything not listed here
/// comes after, alphabetically.
const EXPLORER_ORDER: [&str; 19] = [
    "Workspace",
    "Players",
    "Lighting",
    "MaterialService",
    "ReplicatedFirst",
    "ReplicatedStorage",
    "ServerScriptService",
    "ServerStorage",
    "StarterGui",
    "StarterPack",
    "StarterPlayer",
    "Teams",
    "SoundService",
    "Chat",
    "TextChatService",
    "VoiceChatService",
    "LocalizationService",
    "TestService",
    "VRService",
];

/// Every root "service" class Roblox itself creates in a place file, whether
/// or not Studio's default Explorer view shows it (verified against every
/// root instance dump across this repo's fixtures). A root class absent from
/// this list is some real, user-placed instance, never one of the
/// deliberately-noisy internal ones, so the default filter below must never
/// hide it.
const KNOWN_SERVICES: [&str; 55] = [
    "AssetService",
    "Chat",
    "CollectionService",
    "ContextActionService",
    "CookiesService",
    "CSGDictionaryService",
    "DataStoreService",
    "Debris",
    "DevPackages",
    "GamePassService",
    "GuidRegistryService",
    "HttpService",
    "InsertService",
    "Instance",
    "Lighting",
    "LocalizationService",
    "LodDataService",
    "LuaWebService",
    "MaterialService",
    "NonReplicatedCSGDictionaryService",
    "Packages",
    "PermissionsService",
    "PhysicsService",
    "PlayerEmulatorService",
    "Players",
    "ProcessInstancePhysicsService",
    "ProximityPromptService",
    "ReplicatedFirst",
    "ReplicatedStorage",
    "ScriptService",
    "Selection",
    "SerializationService",
    "ServerPackages",
    "ServerScriptService",
    "ServerStorage",
    "ServiceVisibilityService",
    "SoundService",
    "StarterGui",
    "StarterPack",
    "StarterPlayer",
    "StudioData",
    "Teams",
    "TeleportService",
    "TestService",
    "TextChatService",
    "TimerService",
    "TouchInputService",
    "TweenService",
    "UGCAvatarService",
    "VideoCaptureService",
    "VideoService",
    "VirtualInputManager",
    "VoiceChatService",
    "VRService",
    "Workspace",
];

/// Services Studio's Explorer never lists, not even through Insert Service:
/// each has an editor of its own (`AvatarSettings` is reached from Avatar ›
/// Settings). Checked in Studio against a place holding one.
const NEVER_LISTED: [&str; 1] = ["AvatarSettings"];

/// Where `class` sits in [`SERVICE_ORDER`], if it is one of those.
pub fn rank(class: &str) -> Option<usize> {
    SERVICE_ORDER.iter().position(|service| *service == class)
}

/// Where a root of `class` sits in the Explorer's root list (see
/// [`EXPLORER_ORDER`]), if it has a fixed place there.
pub fn explorer_rank(class: &str) -> Option<usize> {
    EXPLORER_ORDER.iter().position(|service| *service == class)
}

/// Whether a root of this class is one of the services this Explorer knows,
/// shown or not: listed here, or tagged `Service` in the API dump, which
/// is how a service Roblox adds after this list was written
/// (`AvatarSettings`, `ConfigureServerService`) stays out of the default
/// view instead of reading as a user's own root. The list stays because a
/// few (`Packages`, `SerializationService`) are not in the dump at all.
pub fn is_known_service(class: &str) -> bool {
    rank(class).is_some()
        || KNOWN_SERVICES.contains(&class)
        || rbx_reflection::ReflectionDatabase::shared().is_service(class)
}

/// Whether Studio lists a root of this class in any view at all, see
/// [`NEVER_LISTED`].
pub fn is_ever_listed(class: &str) -> bool {
    !NEVER_LISTED.contains(&class)
}

/// Whether Studio's default Explorer view shows a root of this class without
/// the "show all services" toggle: its 14 fixed services, or anything that is
/// not one of Roblox's own well-known service classes at all.
pub fn is_default_visible(class: &str) -> bool {
    rank(class).is_some() || !is_known_service(class)
}
