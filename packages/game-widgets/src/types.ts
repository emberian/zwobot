// Submessage data received from freeform widget
export interface SubmessageData {
  submessage_id: number;
  sender_id: number;
  msg_type: string;
  content: unknown;
}

// Widget context passed from freeform widget
export interface WidgetContext {
  message_id: number;
  post_interaction: (data: Record<string, unknown>) => void;
  post_submessage?: (data: Record<string, unknown>) => Promise<void>;
  on_submessage?: (callback: (data: SubmessageData) => void) => void;
  on: (event: string, selector: string, handler: (e: Event) => void) => void;
  update_html: (html: string) => void;
  // Current user info for attribution
  current_user?: {
    user_id: number;
    full_name: string;
    avatar_url?: string;
  };
  // Initial submessages that existed when widget was rendered
  initial_submessages?: SubmessageData[];
}

// Dialogue component props
export interface DialogueProps {
  speaker?: {
    name: string;
    portraitUrl?: string;
    mood?: 'neutral' | 'happy' | 'angry' | 'sad' | 'surprised';
  };
  text: string;
  choices?: Array<{
    label: string;
    customId: string;
    disabled?: boolean;
    requirementText?: string;
  }>;
  continueCustomId?: string;
  history?: Array<{
    speaker?: string;
    text: string;
  }>;
}

// Room component props
export interface RoomProps {
  name: string;
  description: string;
  imageUrl?: string;
  exits?: Array<{
    direction: string;
    customId: string;
    destinationHint?: string;
    locked?: boolean;
  }>;
  items?: Array<{
    name: string;
    customId: string;
    icon?: string;
  }>;
  npcs?: Array<{
    name: string;
    customId: string;
    portraitUrl?: string;
    status?: 'friendly' | 'neutral' | 'hostile';
  }>;
  actions?: Array<{
    label: string;
    customId: string;
  }>;
}

// Combat component props
export interface CombatProps {
  title?: string;
  participants: Array<{
    id: string;
    name: string;
    portraitUrl?: string;
    team?: 'player' | 'ally' | 'enemy';
    hp: { current: number; max: number };
    isCurrentTurn?: boolean;
    statusEffects?: string[];
  }>;
  actions?: Array<{
    label: string;
    customId: string;
    disabled?: boolean;
    disabledReason?: string;
  }>;
  targets?: Array<{
    id: string;
    name: string;
    customId: string;
  }>;
  log?: string[];
}

// Dice roll component props
export interface DiceRollProps {
  rolls: Array<{
    dice: string;
    results: number[];
    label?: string;
  }>;
  modifier?: number;
  total: number;
  target?: number;
  success?: boolean;
  flavorText?: string;
  animate?: boolean;
}

// Resource bar component props
export interface ResourceBarProps {
  label?: string;
  current: number;
  max: number;
  color?: 'health' | 'mana' | 'stamina' | 'custom';
  customColor?: string;
  showNumbers?: boolean;
  size?: 'sm' | 'md' | 'lg';
  animated?: boolean;
}

// Portrait component props
export interface PortraitProps {
  name: string;
  imageUrl?: string;
  mood?: 'neutral' | 'happy' | 'angry' | 'sad' | 'surprised';
  size?: 'sm' | 'md' | 'lg';
  status?: 'friendly' | 'neutral' | 'hostile';
}

// Transcript entry
export interface TranscriptEntry {
  speaker: string;
  text: string;
  timestamp?: number;
  avatarUrl?: string;
  speakerColor?: string;
}

// Transcript component props - live-updating multi-speaker conversation
export interface TranscriptProps {
  title?: string;
  entries: TranscriptEntry[];
  inputEnabled?: boolean;
  inputPlaceholder?: string;
  inputButtonText?: string;
  showTimestamps?: boolean;
  maxHeight?: number;
  autoScroll?: boolean;
}

// Component type registry
export type ComponentType =
  | 'dialogue'
  | 'room'
  | 'combat'
  | 'diceRoll'
  | 'resourceBar'
  | 'portrait'
  | 'transcript';

export type ComponentProps =
  | DialogueProps
  | RoomProps
  | CombatProps
  | DiceRollProps
  | ResourceBarProps
  | PortraitProps
  | TranscriptProps;
