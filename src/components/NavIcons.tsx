import {
  FolderOpen,
  Bot,
  Play,
  ClipboardList,
  Search,
  BarChart3,
  Radio,
  Brain,
  BookOpen,
  Settings,
  RefreshCw,
} from 'lucide-react';

const SIZE = 16 as const;

type IconProps = {
  className?: string;
  style?: React.CSSProperties;
  'aria-hidden'?: boolean | 'true' | 'false';
};

function withProps(Component: React.ComponentType<{ size?: number; strokeWidth?: number }>, props: IconProps = {}) {
  const { 'aria-hidden': _, ...rest } = props;
  return <Component size={SIZE as number} strokeWidth={1.8} {...rest} />;
}

export const NavIcons = {
  project: (props: IconProps) => withProps(FolderOpen, props),
  agent: (props: IconProps) => withProps(Bot, props),
  runner: (props: IconProps) => withProps(Play, props),
  plans: (props: IconProps) => withProps(ClipboardList, props),
  diagnostics: (props: IconProps) => withProps(Search, props),
  history: (props: IconProps) => withProps(BarChart3, props),
  telemetry: (props: IconProps) => withProps(Radio, props),
  ai: (props: IconProps) => withProps(Brain, props),
  learning: (props: IconProps) => withProps(BookOpen, props),
  settings: (props: IconProps) => withProps(Settings, props),
  updates: (props: IconProps) => withProps(RefreshCw, props),
};
