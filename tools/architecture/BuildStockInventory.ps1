param(
    [Parameter(Mandatory = $true)]
    [string] $EvidenceDirectory,

    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Each rule assigns an observed stock artifact to one architectural owner. The
# first match wins, so narrow feature families must precede broad engine rules.
$sourceRules = @(
    @{ Pattern = '^fmod(_.*)?\.cpp$|^aSfxDsp\.cpp$'; Crate = 'media'; Module = 'audio/backend'; Kind = 'vendor' },
    @{ Pattern = '^(block|codebook|envelope|floor0|floor1|framing|info|mapping0|math|mdct|psy|res0|sharedbook|smallft|vorbisfile)\.c$|^OggDecompress\.cpp$'; Crate = 'media'; Module = 'audio/codec'; Kind = 'vendor' },
    @{ Pattern = '^SoundInterface2VoiceChat\.cpp$'; Crate = 'media'; Module = 'voice'; Kind = 'stock' },
    @{ Pattern = '^SoundInterface2DSP\.cpp$'; Crate = 'media'; Module = 'audio/dsp'; Kind = 'stock' },
    @{ Pattern = '^SoundInterface2ZoneSounds\.cpp$'; Crate = 'media'; Module = 'audio/spatial'; Kind = 'stock' },
    @{ Pattern = '^SoundCache\.h$'; Crate = 'media'; Module = 'audio/cache'; Kind = 'stock' },
    @{ Pattern = '^(PlayerSound_C|UnitSound_C)\.cpp$'; Crate = 'media'; Module = 'audio/emitter'; Kind = 'stock' },
    @{ Pattern = '^(SoundEngine|SoundInterface2|SoundInterface2AdvancedKitProperties|SoundInterface2Internal)\.cpp$'; Crate = 'media'; Module = 'audio/engine'; Kind = 'stock' },
    @{ Pattern = '^(ComSatClient|ComSatSoundIOSoundEngine)\.cpp$'; Crate = 'media'; Module = 'voice'; Kind = 'stock' },
    @{ Pattern = '^CSimpleMovieFrame\.cpp$'; Crate = 'ui'; Module = 'widget/movie'; Kind = 'stock' },

    @{ Pattern = '^(SFile.*|SBig|SCmd|SComp|SSignature|MemoryStorm)\.cpp$'; Crate = 'asset'; Module = 'archive'; Kind = 'vendor' },
    @{ Pattern = '^(Filestack_.*|FileCache)\.cpp$'; Crate = 'asset'; Module = 'file_stack'; Kind = 'stock' },
    @{ Pattern = '^(DBCache|DBCacheInstances|DBClient|WDataStore)\.cpp$|^(CDataStore|WowClientDB)\.h$'; Crate = 'asset'; Module = 'database'; Kind = 'stock' },
    @{ Pattern = '^(blp|tga)\.cpp$|^Texture(Blob|Cache|Int)?\.(cpp|h)$'; Crate = 'asset'; Module = 'texture'; Kind = 'stock' },
    @{ Pattern = '^M2(Cache|Model|Shared)\.cpp$|^ModelBlob\.cpp$'; Crate = 'asset'; Module = 'model'; Kind = 'stock' },
    @{ Pattern = '^Map(Area|Chunk|ChunkLiquid|Load|LowDetail|Mem|Shadow)?\.cpp$'; Crate = 'asset'; Module = 'terrain'; Kind = 'stock' },
    @{ Pattern = '^MapObj(Group|Read)?\.cpp$'; Crate = 'asset'; Module = 'world_model'; Kind = 'stock' },
    @{ Pattern = '^(CDataAllocator|CDataRecycler|IOAlignUnit|IOFileUnit|IOUnitContainer|IObjectAlloc|NewZerofill|cmemblock|lmemPool|stpl)\.(cpp|h)$'; Crate = 'asset'; Module = 'storage'; Kind = 'foundation' },

    @{ Pattern = '^CGx.*\.cpp$'; Crate = 'rendering'; Module = 'device'; Kind = 'stock' },
    @{ Pattern = '^ShaderEffectManager\.cpp$'; Crate = 'rendering'; Module = 'shader'; Kind = 'stock' },
    @{ Pattern = '^(EffectGlow|FFXEffects|Lightning|PassGlow|Prop|CreepTendril|Tumor|TumorManager)\.cpp$'; Crate = 'rendering'; Module = 'effect'; Kind = 'stock' },
    @{ Pattern = '^(M2Light|WorldScene)\.cpp$'; Crate = 'rendering'; Module = 'lighting'; Kind = 'stock' },
    @{ Pattern = '^M2Scene\.cpp$|^(CharacterComponent|CharacterModelBase|ComponentUtils)\.cpp$'; Crate = 'rendering'; Module = 'model'; Kind = 'stock' },
    @{ Pattern = '^ParticleSystem2\.(cpp|h)$'; Crate = 'rendering'; Module = 'particle'; Kind = 'stock' },
    @{ Pattern = '^(WorldFrame|WorldText)\.cpp$'; Crate = 'rendering'; Module = 'scene'; Kind = 'stock' },
    @{ Pattern = '^(DetailDoodad|MapWeather)\.cpp$'; Crate = 'rendering'; Module = 'terrain'; Kind = 'stock' },
    @{ Pattern = '^Camera\.cpp$'; Crate = 'rendering'; Module = 'camera'; Kind = 'stock' },
    @{ Pattern = '^(AaBsp|Collide)\.cpp$'; Crate = 'systems'; Module = 'collision'; Kind = 'stock' },
    @{ Pattern = '^Minimap\.cpp$'; Crate = 'rendering'; Module = 'minimap'; Kind = 'stock' },

    @{ Pattern = '^(Grunt|GruntLogin|BattlenetLogin)\.cpp$'; Crate = 'network'; Module = 'authentication'; Kind = 'stock' },
    @{ Pattern = '^WowConnection\.cpp$'; Crate = 'network'; Module = 'connection'; Kind = 'stock' },
    @{ Pattern = '^WardenClient\.cpp$'; Crate = 'network'; Module = 'integrity'; Kind = 'stock' },
    @{ Pattern = '^Net(Client|Internal)\.(cpp|h)$'; Crate = 'network'; Module = 'session'; Kind = 'stock' },
    @{ Pattern = '^OsTcp\.cpp$'; Crate = 'network'; Module = 'transport'; Kind = 'stock' },
    @{ Pattern = '^(RealmList|Login)\.cpp$'; Crate = 'network'; Module = 'realm'; Kind = 'stock' },
    @{ Pattern = '^AccountData\.cpp$'; Crate = 'network'; Module = 'account_data'; Kind = 'stock' },
    @{ Pattern = '^WowSvcsClientServices\.h$'; Crate = 'network'; Module = 'protocol'; Kind = 'stock' },

    @{ Pattern = '^(Object_C|ObjectAlloc|ObjectMgrClient)\.cpp$'; Crate = 'ecs'; Module = 'object'; Kind = 'stock' },
    @{ Pattern = '^Unit_C\.cpp$'; Crate = 'ecs'; Module = 'unit'; Kind = 'stock' },
    @{ Pattern = '^(Player_C\.(cpp|h)|PlayerName\.cpp)$'; Crate = 'ecs'; Module = 'player'; Kind = 'stock' },
    @{ Pattern = '^CreatureStats\.(cpp|h)$'; Crate = 'ecs'; Module = 'creature'; Kind = 'stock' },
    @{ Pattern = '^GameObject(_C|Stats)\.(cpp|h)$'; Crate = 'ecs'; Module = 'game_object'; Kind = 'stock' },
    @{ Pattern = '^(Bag_C|Item_C)\.cpp$|^(ItemName|ItemStats)\.(cpp|h)$'; Crate = 'ecs'; Module = 'item'; Kind = 'stock' },
    @{ Pattern = '^Corpse_C\.cpp$'; Crate = 'ecs'; Module = 'corpse'; Kind = 'stock' },
    @{ Pattern = '^DynamicObject_C\.cpp$'; Crate = 'ecs'; Module = 'dynamic_object'; Kind = 'stock' },
    @{ Pattern = '^Effect_C\.cpp$'; Crate = 'ecs'; Module = 'effect'; Kind = 'stock' },
    @{ Pattern = '^Missile_C\.cpp$'; Crate = 'ecs'; Module = 'missile'; Kind = 'stock' },
    @{ Pattern = '^Movement_C\.cpp$'; Crate = 'ecs'; Module = 'movement'; Kind = 'stock' },
    @{ Pattern = '^Vehicle_C\.cpp$'; Crate = 'ecs'; Module = 'vehicle'; Kind = 'stock' },
    @{ Pattern = '^Spell_C\.cpp$'; Crate = 'ecs'; Module = 'spell'; Kind = 'stock' },
    @{ Pattern = '^Trade_C\.cpp$'; Crate = 'ecs'; Module = 'trade'; Kind = 'stock' },
    @{ Pattern = '^Minigame_C\.cpp$'; Crate = 'ecs'; Module = 'minigame'; Kind = 'stock' },

    @{ Pattern = '^(Movement|MovementShared|Path)\.cpp$'; Crate = 'systems'; Module = 'movement'; Kind = 'stock' },
    @{ Pattern = '^UnitCombat(_C|Log_C)\.cpp$'; Crate = 'systems'; Module = 'combat'; Kind = 'stock' },
    @{ Pattern = '^UnitMissileTrajectory_C\.cpp$'; Crate = 'systems'; Module = 'missile'; Kind = 'stock' },
    @{ Pattern = '^ObjectEffect\.cpp$'; Crate = 'systems'; Module = 'effect'; Kind = 'stock' },
    @{ Pattern = '^(SpellCast|SpellVisuals)\.cpp$'; Crate = 'systems'; Module = 'spell'; Kind = 'stock' },
    @{ Pattern = '^(UnitVehicle_C|VehiclePassenger_C|Passenger|VehicleCamera_C)\.cpp$'; Crate = 'systems'; Module = 'vehicle'; Kind = 'stock' },
    @{ Pattern = '^AchievementInfo\.cpp$'; Crate = 'systems'; Module = 'achievement'; Kind = 'stock' },
    @{ Pattern = '^ArenaTeamInfo\.cpp$'; Crate = 'systems'; Module = 'arena'; Kind = 'stock' },
    @{ Pattern = '^AuctionHouse\.cpp$'; Crate = 'systems'; Module = 'auction'; Kind = 'stock' },
    @{ Pattern = '^BattlefieldInfo\.(cpp|h)$'; Crate = 'systems'; Module = 'battlefield'; Kind = 'stock' },
    @{ Pattern = '^Calendar(Event)?\.cpp$'; Crate = 'systems'; Module = 'calendar'; Kind = 'stock' },
    @{ Pattern = '^CurrencyTypes\.cpp$'; Crate = 'systems'; Module = 'currency'; Kind = 'stock' },
    @{ Pattern = '^Dance(Cache|Studio)\.(cpp|h)$'; Crate = 'systems'; Module = 'dance'; Kind = 'stock' },
    @{ Pattern = '^DeclinedWords\.cpp$'; Crate = 'systems'; Module = 'language'; Kind = 'stock' },
    @{ Pattern = '^DuelInfo\.cpp$'; Crate = 'systems'; Module = 'duel'; Kind = 'stock' },
    @{ Pattern = '^EquipmentManager\.cpp$'; Crate = 'systems'; Module = 'equipment'; Kind = 'stock' },
    @{ Pattern = '^FriendList\.(cpp|h)$'; Crate = 'systems'; Module = 'social'; Kind = 'stock' },
    @{ Pattern = '^(GMTicketInfo|KnowledgeBase)\.cpp$'; Crate = 'systems'; Module = 'support'; Kind = 'stock' },
    @{ Pattern = '^(GossipInfo|NPCText|PageTextCache)\.(cpp|h)$'; Crate = 'systems'; Module = 'interaction'; Kind = 'stock' },
    @{ Pattern = '^GuildInfo\.cpp$'; Crate = 'systems'; Module = 'guild'; Kind = 'stock' },
    @{ Pattern = '^LFGInfo\.(cpp|h)$'; Crate = 'systems'; Module = 'group_finder'; Kind = 'stock' },
    @{ Pattern = '^LootRoll\.cpp$'; Crate = 'systems'; Module = 'loot'; Kind = 'stock' },
    @{ Pattern = '^MailInfo\.cpp$'; Crate = 'systems'; Module = 'mail'; Kind = 'stock' },
    @{ Pattern = '^NameCache\.(cpp|h)$'; Crate = 'systems'; Module = 'name_cache'; Kind = 'stock' },
    @{ Pattern = '^(PetInfo|PetNameCache)\.(cpp|h)$'; Crate = 'systems'; Module = 'pet'; Kind = 'stock' },
    @{ Pattern = '^(PetitionInfo|PetitionVendor)\.cpp$'; Crate = 'systems'; Module = 'petition'; Kind = 'stock' },
    @{ Pattern = '^Quest(Log|TextParser)\.cpp$'; Crate = 'systems'; Module = 'quest'; Kind = 'stock' },
    @{ Pattern = '^RaidInfo\.cpp$'; Crate = 'systems'; Module = 'raid'; Kind = 'stock' },
    @{ Pattern = '^ReputationInfo\.cpp$'; Crate = 'systems'; Module = 'reputation'; Kind = 'stock' },
    @{ Pattern = '^SkillInfo\.cpp$'; Crate = 'systems'; Module = 'skill'; Kind = 'stock' },
    @{ Pattern = '^StableInfo\.cpp$'; Crate = 'systems'; Module = 'stable'; Kind = 'stock' },
    @{ Pattern = '^TalentInfo\.cpp$'; Crate = 'systems'; Module = 'talent'; Kind = 'stock' },
    @{ Pattern = '^CharacterCreation\.cpp$'; Crate = 'systems'; Module = 'character'; Kind = 'stock' },
    @{ Pattern = '^World(Map|Param)?\.cpp$'; Crate = 'systems'; Module = 'world'; Kind = 'stock' },
    @{ Pattern = '^ItemSocketInfo\.cpp$'; Crate = 'systems'; Module = 'inventory'; Kind = 'stock' },

    @{ Pattern = '^(CGlueMgr|PatchDownloadGlue|ScanDLLGlue|SurveyDownloadGlue)\.cpp$'; Crate = 'ui'; Module = 'glue'; Kind = 'stock' },
    @{ Pattern = '^CScriptRegion(Script)?\.cpp$'; Crate = 'ui'; Module = 'region'; Kind = 'stock' },
    @{ Pattern = '^CSimpleAnim(Script)?\.cpp$'; Crate = 'ui'; Module = 'animation'; Kind = 'stock' },
    @{ Pattern = '^(CSimpleFont|GxuFontMiscClasses|GxuFontString|GxuFontUtil|IGxuFontGlyph)\.cpp$'; Crate = 'ui'; Module = 'font'; Kind = 'stock' },
    @{ Pattern = '^CSimple(Frame|FrameScript|Top)\.cpp$'; Crate = 'ui'; Module = 'frame'; Kind = 'stock' },
    @{ Pattern = '^CSimpleEditBox\.(cpp|h)$'; Crate = 'ui'; Module = 'widget/edit_box'; Kind = 'stock' },
    @{ Pattern = '^CSimpleHTML\.cpp$'; Crate = 'ui'; Module = 'widget/html'; Kind = 'stock' },
    @{ Pattern = '^CSimpleHyperlinkedFrame\.cpp$'; Crate = 'ui'; Module = 'widget/hyperlink'; Kind = 'stock' },
    @{ Pattern = '^CSimpleMessage(Frame|ScrollFrame)\.(cpp|h)$'; Crate = 'ui'; Module = 'widget/message'; Kind = 'stock' },
    @{ Pattern = '^CSimpleRender\.cpp$'; Crate = 'ui'; Module = 'render'; Kind = 'stock' },
    @{ Pattern = '^(SimpleScript|ScriptEvents)\.cpp$'; Crate = 'ui'; Module = 'script'; Kind = 'stock' },
    @{ Pattern = '^XMLTree\.cpp$'; Crate = 'ui'; Module = 'xml'; Kind = 'stock' },
    @{ Pattern = '^AddOns\.cpp$'; Crate = 'ui'; Module = 'addon'; Kind = 'stock' },
    @{ Pattern = '^(UIBindings|UIMacroOptions|UIMacros)\.(cpp|h)$'; Crate = 'ui'; Module = 'binding'; Kind = 'stock' },
    @{ Pattern = '^ActionBarFrame\.cpp$'; Crate = 'ui'; Module = 'feature/action_bar'; Kind = 'stock' },
    @{ Pattern = '^BattlenetUI\.cpp$'; Crate = 'ui'; Module = 'feature/battlenet'; Kind = 'stock' },
    @{ Pattern = '^(ChatBubbleFrame|ChatFrame)\.(cpp|h)$'; Crate = 'ui'; Module = 'feature/chat'; Kind = 'stock' },
    @{ Pattern = '^ClassTrainerFrame\.cpp$'; Crate = 'ui'; Module = 'feature/trainer'; Kind = 'stock' },
    @{ Pattern = '^CommentatorFrame\.cpp$'; Crate = 'ui'; Module = 'feature/commentator'; Kind = 'stock' },
    @{ Pattern = '^ContainerFrame\.cpp$'; Crate = 'ui'; Module = 'feature/container'; Kind = 'stock' },
    @{ Pattern = '^DressUpModelFrame\.cpp$'; Crate = 'ui'; Module = 'feature/dress_up'; Kind = 'stock' },
    @{ Pattern = '^GuildBankFrame\.cpp$'; Crate = 'ui'; Module = 'feature/guild_bank'; Kind = 'stock' },
    @{ Pattern = '^HealthBar\.cpp$'; Crate = 'ui'; Module = 'feature/health_bar'; Kind = 'stock' },
    @{ Pattern = '^ItemTextFrame\.cpp$'; Crate = 'ui'; Module = 'feature/item_text'; Kind = 'stock' },
    @{ Pattern = '^LootFrame\.cpp$'; Crate = 'ui'; Module = 'feature/loot'; Kind = 'stock' },
    @{ Pattern = '^MerchantFrame\.cpp$'; Crate = 'ui'; Module = 'feature/merchant'; Kind = 'stock' },
    @{ Pattern = '^MinimapFrame\.cpp$'; Crate = 'ui'; Module = 'feature/minimap'; Kind = 'stock' },
    @{ Pattern = '^NamePlateFrame\.cpp$'; Crate = 'ui'; Module = 'feature/name_plate'; Kind = 'stock' },
    @{ Pattern = '^PaperDollInfoFrame\.cpp$'; Crate = 'ui'; Module = 'feature/paper_doll'; Kind = 'stock' },
    @{ Pattern = '^PartyFrame\.cpp$'; Crate = 'ui'; Module = 'feature/party'; Kind = 'stock' },
    @{ Pattern = '^PortraitButton\.cpp$'; Crate = 'ui'; Module = 'feature/portrait'; Kind = 'stock' },
    @{ Pattern = '^QuestFrame\.cpp$'; Crate = 'ui'; Module = 'feature/quest'; Kind = 'stock' },
    @{ Pattern = '^SpellBookFrame\.cpp$'; Crate = 'ui'; Module = 'feature/spell_book'; Kind = 'stock' },
    @{ Pattern = '^TaxiMapFrame\.cpp$'; Crate = 'ui'; Module = 'feature/taxi_map'; Kind = 'stock' },
    @{ Pattern = '^Tooltip\.cpp$'; Crate = 'ui'; Module = 'feature/tooltip'; Kind = 'stock' },
    @{ Pattern = '^TradeFrame\.cpp$'; Crate = 'ui'; Module = 'feature/trade'; Kind = 'stock' },
    @{ Pattern = '^TradeSkillFrame\.cpp$'; Crate = 'ui'; Module = 'feature/trade_skill'; Kind = 'stock' },
    @{ Pattern = '^GameUI\.cpp$'; Crate = 'ui'; Module = 'world'; Kind = 'stock' },
    @{ Pattern = '^SysMessage\.cpp$'; Crate = 'ui'; Module = 'notification'; Kind = 'stock' },

    @{ Pattern = '^(Client|ClientServices)\.cpp$'; Crate = 'runtime'; Module = 'application'; Kind = 'stock' },
    @{ Pattern = '^(Profile|Status)\.(cpp|h)$'; Crate = 'runtime'; Module = 'configuration'; Kind = 'stock' },
    @{ Pattern = '^Console(Client|Detect|Var)\.cpp$'; Crate = 'runtime'; Module = 'console'; Kind = 'stock' },
    @{ Pattern = '^(EvtSched|EvtTimer|SEvt)\.cpp$'; Crate = 'runtime'; Module = 'event'; Kind = 'stock' },
    @{ Pattern = '^InputControl\.(cpp|h)$'; Crate = 'runtime'; Module = 'input'; Kind = 'stock' },
    @{ Pattern = '^LoadingScreen\.cpp$'; Crate = 'runtime'; Module = 'loading'; Kind = 'stock' },
    @{ Pattern = '^TimeManager\.cpp$'; Crate = 'runtime'; Module = 'time'; Kind = 'stock' },
    @{ Pattern = '^Os(?!Tcp).*\.cpp$|^(BlizzardCursor|Cursor)\.(c|cpp)$'; Crate = 'runtime'; Module = 'platform'; Kind = 'platform' },
    @{ Pattern = '^(EZ_LCD_Page|EZ_LCD|LCD|LCDGfxBase|hidmanagerimpl|asiolist|LAYER)\.cpp$'; Crate = 'runtime'; Module = 'platform/lcd'; Kind = 'vendor' },
    @{ Pattern = '^(CDataRecycler|DynamicString|RCString|String|MeteredSection)\.cpp$|^(HashMap|Repair|termination|tos|x)\.h$'; Crate = 'runtime'; Module = 'foundation'; Kind = 'foundation' },
    @{ Pattern = '^(SLock|SThread)\.cpp$'; Crate = 'cpu'; Module = 'synchronization'; Kind = 'foundation' },
    @{ Pattern = '^GfxSingletonManager\.cpp$'; Crate = 'rendering'; Module = 'device'; Kind = 'foundation' },
    @{ Pattern = '^(CheckExecutableSignature|DRMHeader)\.(cpp|C)$|^PatchFiles\.h$'; Crate = 'runtime'; Module = 'security'; Kind = 'stock' },
    @{ Pattern = '^(credits|credits_BC|credits_LK|eula|Zones)\.h$'; Crate = 'runtime'; Module = 'legal'; Kind = 'data' },
    @{ Pattern = '^(cn\.kbase\.blizzard|eu\.tracker\.worldofwarcraft|support\.worldofwarcraft|support\.wow-europe|support\.wowtaiwan|us\.logon\.worldofwarcraft|us\.tracker\.worldofwarcraft|www\.memtest86)\.c$'; Crate = 'runtime'; Module = 'telemetry'; Kind = 'embedded' },
    @{ Pattern = '.*'; Crate = ''; Module = ''; Kind = 'unmapped' }
)

# RTTI rules are deliberately family-based. Domain-bearing names are matched
# before generic container and allocator templates so their ownership is not
# hidden by an implementation-detail classification.
$rttiRules = @(
    @{ Pattern = 'CAsyncQueue|CAsyncThread'; Crate = 'cpu'; Module = 'job'; Kind = 'foundation' },
    @{ Pattern = 'MAPDATA|CMapAreaTexture|CMapChunkBuf|CMapDoodadDef|CMapFootprintTexture|CMapObj|CComponentMipBits|CMipBitsCache|CHashEntry@CModelBlob|CACHEENTRY|Data@Chunk|MINIMAPMD5NAME'; Crate = 'asset'; Module = 'world'; Kind = 'stock' },
    @{ Pattern = 'BANNEDADDON|ADDONSTATE|SAVEDVARIABLE|METADATA@UIADDON'; Crate = 'ui'; Module = 'addon'; Kind = 'stock' },
    @{ Pattern = 'AUTOCOMPLETE|CHARACTER_INFO|CharacterSelectionDisplay'; Crate = 'ui'; Module = 'glue/character'; Kind = 'stock' },
    @{ Pattern = 'MapInfo@CGCommentator'; Crate = 'ui'; Module = 'feature/commentator'; Kind = 'stock' },
    @{ Pattern = 'CGSimpleHealthBar'; Crate = 'ui'; Module = 'feature/health_bar'; Kind = 'stock' },
    @{ Pattern = 'CGChatBubbleFrame'; Crate = 'ui'; Module = 'feature/chat'; Kind = 'stock' },
    @{ Pattern = 'CGQuestPOIFrame|QuestPOIRenderData'; Crate = 'ui'; Module = 'feature/quest'; Kind = 'stock' },
    @{ Pattern = 'GuildBankItem|GUILDUPDATETIMESTAMP'; Crate = 'ui'; Module = 'feature/guild_bank'; Kind = 'stock' },
    @{ Pattern = 'THREATMAP@CGGameUI'; Crate = 'ui'; Module = 'world'; Kind = 'stock' },
    @{ Pattern = 'BITMAPBYICON|CHARCODEDESC|KERN|SIMPLEANIMNODE|SIMPLEFONT'; Crate = 'ui'; Module = 'font'; Kind = 'stock' },
    @{ Pattern = 'MODIFIEDCLICK|KEYCOMMAND|SimpleScriptFunction|WORDLIST'; Crate = 'ui'; Module = 'binding'; Kind = 'stock' },
    @{ Pattern = 'CLCDBASE_NODE|LCDHANDLE'; Crate = 'runtime'; Module = 'platform/lcd'; Kind = 'vendor' },
    @{ Pattern = 'CB_EVENT_NODE@CLCDConnection'; Crate = 'runtime'; Module = 'platform/lcd'; Kind = 'vendor' },
    @{ Pattern = 'CMDDEF|CONSOLECOMMAND|CONSOLELINE|ConsoleString|CVar'; Crate = 'runtime'; Module = 'console'; Kind = 'stock' },
    @{ Pattern = 'EVENTCALLBACKREG|EVENTDATEHASH|EVENTDISPATCHREG|EVENTLISTENERNODE|EventReg|EvtKeyDown|EvtMessage|EvtThread'; Crate = 'runtime'; Module = 'event'; Kind = 'stock' },
    @{ Pattern = 'KEYVALUE@ProfileInternal|SECTION@ProfileInternal|STRINGBLOCK@ProfileInternal|STATUSENTRY'; Crate = 'runtime'; Module = 'configuration'; Kind = 'stock' },
    @{ Pattern = 'CMirrorHandler|GAMETIMECBSTRUCT|TimeEvent|TIMESTAMPSTRUCT'; Crate = 'runtime'; Module = 'time'; Kind = 'stock' },
    @{ Pattern = 'OsIMECandidate|W32Joystick|WNDREC'; Crate = 'runtime'; Module = 'platform'; Kind = 'platform' },
    @{ Pattern = 'OUTPUT@OsNet|PENDINGINVITENODE|PENDINGTEXTEMOTE|PENDINGUSERLIST|PROFANITYCHECK|SPAMCHECK|REALM_INFO'; Crate = 'network'; Module = 'session'; Kind = 'stock' },
    @{ Pattern = 'SEChannelGroup|SEDriverInfo|SGroupPtr'; Crate = 'media'; Module = 'audio/backend'; Kind = 'stock' },
    @{ Pattern = 'ACHIEVEMENT|Achievement|SPECIFIC_CRITERIA'; Crate = 'systems'; Module = 'achievement'; Kind = 'stock' },
    @{ Pattern = 'Auction'; Crate = 'systems'; Module = 'auction'; Kind = 'stock' },
    @{ Pattern = 'CALENDAR|Calendar|HOLIDAY'; Crate = 'systems'; Module = 'calendar'; Kind = 'stock' },
    @{ Pattern = 'Chat|CHAT'; Crate = 'systems'; Module = 'social'; Kind = 'stock' },
    @{ Pattern = 'Combat|COMBAT|Encounter|ENCOUNTER|Threat|THREAT'; Crate = 'systems'; Module = 'combat'; Kind = 'stock' },
    @{ Pattern = 'DECLINEDWORD'; Crate = 'systems'; Module = 'language'; Kind = 'stock' },
    @{ Pattern = 'DANCE|Dance'; Crate = 'systems'; Module = 'dance'; Kind = 'stock' },
    @{ Pattern = 'FACTION|Reputation'; Crate = 'systems'; Module = 'reputation'; Kind = 'stock' },
    @{ Pattern = 'Guild'; Crate = 'systems'; Module = 'guild'; Kind = 'stock' },
    @{ Pattern = 'LFG|LookingForGroup'; Crate = 'systems'; Module = 'group_finder'; Kind = 'stock' },
    @{ Pattern = 'Loot'; Crate = 'systems'; Module = 'loot'; Kind = 'stock' },
    @{ Pattern = 'PetAction'; Crate = 'systems'; Module = 'pet'; Kind = 'stock' },
    @{ Pattern = 'Petition'; Crate = 'systems'; Module = 'petition'; Kind = 'stock' },
    @{ Pattern = 'QUEST|Quest'; Crate = 'systems'; Module = 'quest'; Kind = 'stock' },
    @{ Pattern = 'Raid'; Crate = 'systems'; Module = 'raid'; Kind = 'stock' },
    @{ Pattern = 'SKILL|Skill|Talent'; Crate = 'systems'; Module = 'talent'; Kind = 'stock' },
    @{ Pattern = 'TAXI|Taxi|ShipPath'; Crate = 'systems'; Module = 'movement'; Kind = 'stock' },
    @{ Pattern = 'Trade'; Crate = 'systems'; Module = 'inventory'; Kind = 'stock' },
    @{ Pattern = 'Aura|AURA|Spell|SPELL|PROFICIENCY|CGCooldown'; Crate = 'systems'; Module = 'spell'; Kind = 'stock' },
    @{ Pattern = 'Battlefield'; Crate = 'systems'; Module = 'battlefield'; Kind = 'stock' },
    @{ Pattern = 'Equipment'; Crate = 'systems'; Module = 'equipment'; Kind = 'stock' },
    @{ Pattern = 'DYNAMICHOLIDAYHASH|PeriodicClientTrigger|POIDIRECTIONDATA|POIIcon|POIINFO|WORLDSTATE|WorldMap'; Crate = 'systems'; Module = 'world'; Kind = 'stock' },
    @{ Pattern = 'RACECLASSINFO|CComponentRequest'; Crate = 'systems'; Module = 'character'; Kind = 'stock' },
    @{ Pattern = 'TEXTEMOTELOOKUP'; Crate = 'systems'; Module = 'interaction'; Kind = 'stock' },
    @{ Pattern = 'InvalidatedName'; Crate = 'systems'; Module = 'name_cache'; Kind = 'stock' },
    @{ Pattern = 'FACEDATA|FOOTSTEPSNDCACHE'; Crate = 'systems'; Module = 'character'; Kind = 'stock' },
    @{ Pattern = 'OBJ_EFFECT_|CDestructibleProxy'; Crate = 'systems'; Module = 'effect'; Kind = 'stock' },
    @{ Pattern = 'Leg@ShipPath|Segment@ShipPath|RouteLines'; Crate = 'systems'; Module = 'movement'; Kind = 'stock' },
    @{ Pattern = 'C2iVector|C2Vector|C3iVector|C3Vector|C4Plane|CImVector|CRect@NTempest|CFacet@NTempest'; Crate = 'rendering'; Module = 'math'; Kind = 'foundation' },
    @{ Pattern = 'Block@FVBBList|Block@VBBList|CClipVolume|CPortalView|DIRTYFACE|DYNAMICELEMENTVERT|GRADIENTINFO|GxDrawListEntry|RECTF|RGN|VERT|CBarrier|CDetailDoodadInstAdd|CRibbonMat'; Crate = 'rendering'; Module = 'geometry'; Kind = 'stock' },
    @{ Pattern = 'DNOverrideSky|Mist@Mists'; Crate = 'rendering'; Module = 'weather'; Kind = 'stock' },
    @{ Pattern = 'CModelRecord|CGCharacterModelBase|SWModelFadeout'; Crate = 'rendering'; Module = 'model'; Kind = 'stock' },
    @{ Pattern = 'CShaderEffect|USEABLESTYLE|STYLEREC'; Crate = 'rendering'; Module = 'shader'; Kind = 'stock' },
    @{ Pattern = 'GXUEMBEDDEDTEXTUREINFO|TEXTLINETEXTURE'; Crate = 'rendering'; Module = 'texture'; Kind = 'stock' },
    @{ Pattern = 'SWorldTextIcon|WORLDTEXTSTRING'; Crate = 'rendering'; Module = 'world_text'; Kind = 'stock' },
    @{ Pattern = 'HANDLER|MessageData|OBJINFO|PrefetchNode|QueueNode@CGCommentator|Shard|SWING'; Crate = 'runtime'; Module = 'foundation'; Kind = 'foundation' },
    @{ Pattern = 'Liquid|Water|Magma'; Crate = 'rendering'; Module = 'liquid'; Kind = 'stock' },
    @{ Pattern = 'CGDressUpModelFrame|CGTabardModelFrame'; Crate = 'ui'; Module = 'feature/dress_up'; Kind = 'stock' },
    @{ Pattern = 'CGMinimapFrame'; Crate = 'ui'; Module = 'feature/minimap'; Kind = 'stock' },
    @{ Pattern = 'CGNamePlateFrame'; Crate = 'ui'; Module = 'feature/name_plate'; Kind = 'stock' },
    @{ Pattern = 'CGTooltip'; Crate = 'ui'; Module = 'feature/tooltip'; Kind = 'stock' },
    @{ Pattern = 'CGWorldFrame|CapturePointUIManagerNode'; Crate = 'ui'; Module = 'world'; Kind = 'stock' },
    @{ Pattern = 'CSimpleAnim|CSimpleControlPoint|SIMPLEANIMNODE'; Crate = 'ui'; Module = 'animation'; Kind = 'stock' },
    @{ Pattern = 'CSimpleButton'; Crate = 'ui'; Module = 'widget/button'; Kind = 'stock' },
    @{ Pattern = 'CSimpleCheckbox'; Crate = 'ui'; Module = 'widget/check_box'; Kind = 'stock' },
    @{ Pattern = 'CSimpleColorSelect'; Crate = 'ui'; Module = 'widget/color_select'; Kind = 'stock' },
    @{ Pattern = 'CSimpleEditBox|EditHistory'; Crate = 'ui'; Module = 'widget/edit_box'; Kind = 'stock' },
    @{ Pattern = 'CSimpleHTML'; Crate = 'ui'; Module = 'widget/html'; Kind = 'stock' },
    @{ Pattern = 'CSimpleHyperlink'; Crate = 'ui'; Module = 'widget/hyperlink'; Kind = 'stock' },
    @{ Pattern = 'CSimpleMessage'; Crate = 'ui'; Module = 'widget/message'; Kind = 'stock' },
    @{ Pattern = 'CSimpleModel'; Crate = 'ui'; Module = 'widget/model'; Kind = 'stock' },
    @{ Pattern = 'CSimpleMovieFrame|MOVIECAPTION'; Crate = 'ui'; Module = 'widget/movie'; Kind = 'stock' },
    @{ Pattern = 'CSimpleScrollFrame'; Crate = 'ui'; Module = 'widget/scroll_frame'; Kind = 'stock' },
    @{ Pattern = 'CSimpleSlider'; Crate = 'ui'; Module = 'widget/slider'; Kind = 'stock' },
    @{ Pattern = 'CSimpleStatusBar'; Crate = 'ui'; Module = 'widget/status_bar'; Kind = 'stock' },
    @{ Pattern = 'CSimpleEmbeddedTexture|CSimpleTexture'; Crate = 'ui'; Module = 'widget/texture'; Kind = 'stock' },
    @{ Pattern = 'CSimpleFontString|Glyph|Font|FONTHASH|BATCHEDRENDERFONTDESC|GXUFONTHYPERLINKINFO|CGxFont'; Crate = 'ui'; Module = 'font'; Kind = 'stock' },
    @{ Pattern = 'CSimpleFrame|CLayoutFrame|CFramePoint|FRAMEATTR|FrameFactoryNode|FRAMENODE|FrameScript|FrameStackInfo|SIMPLEFRAMENODE|CLICKFRAME'; Crate = 'ui'; Module = 'frame'; Kind = 'stock' },
    @{ Pattern = 'CScriptRegion'; Crate = 'ui'; Module = 'region'; Kind = 'stock' },
    @{ Pattern = 'CGlue'; Crate = 'ui'; Module = 'glue'; Kind = 'stock' },
    @{ Pattern = 'XMLNode|CXMLAttribute'; Crate = 'ui'; Module = 'xml'; Kind = 'stock' },
    @{ Pattern = 'Taint'; Crate = 'ui'; Module = 'script'; Kind = 'stock' },
    @{ Pattern = 'Macro|MACRO|KEYBINDING|MOUSELOOK|OVERRIDEKEYBINDING'; Crate = 'ui'; Module = 'binding'; Kind = 'stock' },
    @{ Pattern = 'UIADDON'; Crate = 'ui'; Module = 'addon'; Kind = 'stock' },
    @{ Pattern = 'InstanceInfo@CGCommentator'; Crate = 'ui'; Module = 'feature/commentator'; Kind = 'stock' },
    @{ Pattern = 'INVENTORYART|ITEMBYNAME|ITEMCOOLDOWNHASHNODE|ITEMPORTRAIT|ITEMSWAP|EQUIPMENT_SET_NODE|InitialSpellStruct'; Crate = 'ui'; Module = 'feature/paper_doll'; Kind = 'stock' },
    @{ Pattern = 'DBCache|REVERSEENTRY'; Crate = 'asset'; Module = 'database'; Kind = 'stock' },
    @{ Pattern = 'CSimpleBatchedMesh'; Crate = 'ui'; Module = 'render'; Kind = 'stock' },
    @{ Pattern = 'CGx|CTexture|CM2|M2|Particle|Frustum|Occlud|Light|Fog|Lightning|Liquid|Water|Rain|Snow|Sand|Render|Camera'; Crate = 'rendering'; Module = 'scene'; Kind = 'stock' },
    @{ Pattern = 'Sound|SI2|ComSat|DSP|SOUNDKIT|ZONESOUND|Voice'; Crate = 'media'; Module = 'audio'; Kind = 'stock' },
    @{ Pattern = 'Battlenet|Grunt|NetClient|NETEVENT|RealmConnection|Warden|HELDMESSAGE|Packet'; Crate = 'network'; Module = 'session'; Kind = 'stock' },
    @{ Pattern = 'DBCache|CacheData|CStreaming|M2Element|TEXTURECACHE|CData'; Crate = 'asset'; Module = 'cache'; Kind = 'stock' },
    @{ Pattern = 'LOADINGSCREEN'; Crate = 'runtime'; Module = 'loading'; Kind = 'stock' },
    @{ Pattern = 'MUTEDPLAYER|TALKINGPLAYER'; Crate = 'media'; Module = 'voice'; Kind = 'stock' },
    @{ Pattern = 'PLAYERPORTRAIT|UNITPORTRAIT'; Crate = 'ui'; Module = 'feature/portrait'; Kind = 'stock' },
    @{ Pattern = 'CPlayerMoveEvent|MountTransitionObject|PosDelta'; Crate = 'systems'; Module = 'movement'; Kind = 'stock' },
    @{ Pattern = 'VehiclePassengerRescueTransition|Passenger'; Crate = 'systems'; Module = 'vehicle'; Kind = 'stock' },
    @{ Pattern = 'PlayerForcedReaction|PlayerPendingItemExpiration|PLAYERNAMEDESC|Player|PLAYER'; Crate = 'ecs'; Module = 'player'; Kind = 'stock' },
    @{ Pattern = 'NearestUnitData|Unit|UNIT'; Crate = 'ecs'; Module = 'unit'; Kind = 'stock' },
    @{ Pattern = 'CObjectEffect'; Crate = 'systems'; Module = 'effect'; Kind = 'stock' },
    @{ Pattern = 'BlizzardObject|CGObject_C|CAsyncObject|CObjectHeap|WTOBJECT|Object|OBJECT'; Crate = 'ecs'; Module = 'object'; Kind = 'stock' },
    @{ Pattern = 'Vehicle'; Crate = 'ecs'; Module = 'vehicle'; Kind = 'stock' },
    @{ Pattern = 'ITEM|Item'; Crate = 'ecs'; Module = 'item'; Kind = 'stock' },
    @{ Pattern = 'Creature'; Crate = 'ecs'; Module = 'creature'; Kind = 'stock' },
    @{ Pattern = 'Corpse'; Crate = 'ecs'; Module = 'corpse'; Kind = 'stock' },
    @{ Pattern = 'Missile'; Crate = 'ecs'; Module = 'missile'; Kind = 'stock' },
    @{ Pattern = 'Movement'; Crate = 'ecs'; Module = 'movement'; Kind = 'stock' },
    @{ Pattern = '\?\$TSExplicit|std@@|type_info|regex_t|HashedNode|BFSNODE|SOURCE@@|FOUNDPARAM|UncachableNode|UniqueSignal|DictionaryRecord|CONTENTNODE'; Crate = 'runtime'; Module = 'foundation'; Kind = 'foundation' },
    @{ Pattern = '.*'; Crate = ''; Module = ''; Kind = 'unmapped' }
)

# Imported DLLs describe platform and dependency boundaries rather than C++
# source ownership. A namespace is assigned to its primary architectural owner;
# individual calls remain available in the generated table for later analysis.
$importRules = @(
    @{ Pattern = '^OPENGL32\.DLL$'; Crate = 'rendering'; Module = 'device'; Kind = 'stock-backend' },
    @{ Pattern = '^DIVXDECODER\.DLL$'; Crate = 'media'; Module = 'cinematic'; Kind = 'vendor' },
    @{ Pattern = '^MSACM32\.DLL$|^WINMM\.DLL$'; Crate = 'media'; Module = 'audio/backend'; Kind = 'platform' },
    @{ Pattern = '^WS2_32\.DLL$|^WININET\.DLL$'; Crate = 'network'; Module = 'transport'; Kind = 'platform' },
    @{ Pattern = '^DINPUT8\.DLL$|^IMM32\.DLL$'; Crate = 'runtime'; Module = 'input'; Kind = 'platform' },
    @{ Pattern = '^ADVAPI32\.DLL$'; Crate = 'runtime'; Module = 'security'; Kind = 'platform' },
    @{ Pattern = '^HID\.DLL$'; Crate = 'runtime'; Module = 'platform/lcd'; Kind = 'platform' },
    @{ Pattern = '^GDI32\.DLL$|^KERNEL32\.DLL$|^OLE32\.DLL$|^SETUPAPI\.DLL$|^SHELL32\.DLL$|^USER32\.DLL$|^VERSION\.DLL$'; Crate = 'runtime'; Module = 'platform'; Kind = 'platform' },
    @{ Pattern = '.*'; Crate = ''; Module = ''; Kind = 'unmapped' }
)

function Resolve-Owner {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Name,

        [Parameter(Mandatory = $true)]
        [array] $Rules
    )

    foreach ($rule in $Rules) {
        if ($Name -match $rule.Pattern) {
            return $rule
        }
    }

    throw "rule table has no terminal rule for $Name"
}

$sourcePath = Join-Path $EvidenceDirectory 'source_files.tsv'
$rttiPath = Join-Path $EvidenceDirectory 'rtti_types.tsv'
$importPath = Join-Path $EvidenceDirectory 'imports.tsv'
if (
    -not (Test-Path -LiteralPath $sourcePath) -or
    -not (Test-Path -LiteralPath $rttiPath) -or
    -not (Test-Path -LiteralPath $importPath)
) {
    throw 'evidence directory does not contain the expected Ghidra reports'
}

$sourceEvidence = Import-Csv -Delimiter "`t" -LiteralPath $sourcePath
$rttiEvidence = Import-Csv -Delimiter "`t" -LiteralPath $rttiPath
$importEvidence = Import-Csv -Delimiter "`t" -LiteralPath $importPath

$sourceInventory = foreach ($group in ($sourceEvidence | Group-Object source_file)) {
    $owner = Resolve-Owner -Name $group.Name -Rules $sourceRules
    [pscustomobject]@{
        artifact = $group.Name
        crate = $owner.Crate
        module = $owner.Module
        disposition = $owner.Kind
        evidence_rows = $group.Count
        cross_references = @($group.Group | Where-Object xref_address).Count
    }
}

$rttiInventory = foreach ($row in $rttiEvidence) {
    $owner = Resolve-Owner -Name $row.rtti_name -Rules $rttiRules
    [pscustomobject]@{
        artifact = $row.rtti_name
        crate = $owner.Crate
        module = $owner.Module
        disposition = $owner.Kind
        addresses = $row.addresses
    }
}

$importInventory = foreach ($row in $importEvidence) {
    $owner = Resolve-Owner -Name $row.namespace -Rules $importRules
    [pscustomobject]@{
        artifact = "$($row.namespace)!$($row.symbol)"
        crate = $owner.Crate
        module = $owner.Module
        disposition = $owner.Kind
    }
}

$unmappedSources = @($sourceInventory | Where-Object disposition -eq 'unmapped')
$unmappedRtti = @($rttiInventory | Where-Object disposition -eq 'unmapped')
$unmappedImports = @($importInventory | Where-Object disposition -eq 'unmapped')

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$sourceInventory |
    Sort-Object artifact |
    Export-Csv -Delimiter "`t" -NoTypeInformation -LiteralPath (Join-Path $OutputDirectory 'stock-source-ownership.tsv')
$rttiInventory |
    Sort-Object artifact |
    Export-Csv -Delimiter "`t" -NoTypeInformation -LiteralPath (Join-Path $OutputDirectory 'stock-rtti-ownership.tsv')
$importInventory |
    Sort-Object artifact |
    Export-Csv -Delimiter "`t" -NoTypeInformation -LiteralPath (Join-Path $OutputDirectory 'stock-import-ownership.tsv')

[pscustomobject]@{
    source_artifacts = $sourceInventory.Count
    mapped_source_artifacts = $sourceInventory.Count - $unmappedSources.Count
    rtti_artifacts = $rttiInventory.Count
    mapped_rtti_artifacts = $rttiInventory.Count - $unmappedRtti.Count
    import_artifacts = $importInventory.Count
    mapped_import_artifacts = $importInventory.Count - $unmappedImports.Count
    unmapped_source_artifacts = $unmappedSources.Count
    unmapped_rtti_artifacts = $unmappedRtti.Count
    unmapped_import_artifacts = $unmappedImports.Count
} | Format-List

if ($unmappedSources.Count -gt 0) {
    'Unmapped source artifacts:'
    $unmappedSources.artifact | Sort-Object
}
if ($unmappedRtti.Count -gt 0) {
    'Unmapped RTTI artifacts:'
    $unmappedRtti.artifact | Sort-Object
}
if ($unmappedImports.Count -gt 0) {
    'Unmapped import artifacts:'
    $unmappedImports.artifact | Sort-Object
}
if ($unmappedSources.Count -gt 0 -or $unmappedRtti.Count -gt 0 -or $unmappedImports.Count -gt 0) {
    exit 1
}
