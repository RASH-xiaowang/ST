<script lang="ts">
  import {
    SquaresFour, Files, ChatCircleDots, BookOpenText, ClockCounterClockwise, Database,
    Plus, PencilSimple, Trash, Folder, FolderPlus, FolderOpen, FileText, MagnifyingGlass,
    GearSix, UploadSimple, DownloadSimple, ArrowsClockwise, X, CaretRight, CaretDown,
    CheckCircle, WarningCircle, Info, PaperPlaneTilt, LinkSimple, Graph, Sparkle,
    ArrowLeft, ArrowRight, ArrowUp, ArrowDown, Stack, CompassRose, ShieldCheck, ListBullets, Scroll,
    FileArrowUp, Swap, SlidersHorizontal, CircleNotch, Lightbulb, FileDashed, FileXls, Tray,
    Cube, ChartBar, FilePdf, FileDoc, FileCsv, FileMd, ChatCircle, PushPin, Minus, ArrowsOut,
    Eye, Tag, DotsThree, Book, Lightning, House,
  } from 'phosphor-svelte';

  const ICONS: Record<string, typeof FileText> = {
    dashboard: SquaresFour,
    docs: Files,
    chat: ChatCircleDots,
    wiki: BookOpenText,
    activity: ClockCounterClockwise,
    kb: Database,
    plus: Plus,
    edit: PencilSimple,
    trash: Trash,
    folder: Folder,
    folderPlus: FolderPlus,
    folderOpen: FolderOpen,
    file: FileText,
    search: MagnifyingGlass,
    settings: GearSix,
    upload: UploadSimple,
    download: DownloadSimple,
    refresh: ArrowsClockwise,
    close: X,
    caretRight: CaretRight,
    caretDown: CaretDown,
    check: CheckCircle,
    warn: WarningCircle,
    info: Info,
    send: PaperPlaneTilt,
    link: LinkSimple,
    graph: Graph,
    sparkle: Sparkle,
    arrowLeft: ArrowLeft,
    arrowRight: ArrowRight,
    arrowUp: ArrowUp,
    arrowDown: ArrowDown,
    stack: Stack,
    compass: CompassRose,
    shield: ShieldCheck,
    list: ListBullets,
    scroll: Scroll,
    fileUp: FileArrowUp,
    move: Swap,
    sliders: SlidersHorizontal,
    loading: CircleNotch,
    idea: Lightbulb,
    fileDashed: FileDashed,
    tray: Tray,
    cube: Cube,
    chart: ChartBar,
    filePdf: FilePdf,
    fileDoc: FileDoc,
    fileCsv: FileCsv,
    fileMd: FileMd,
    fileXlsx: FileXls,
    chatCircle: ChatCircle,
    pin: PushPin,
    eye: Eye,
    tag: Tag,
    dotsThree: DotsThree,
    minus: Minus,
    arrowsOut: ArrowsOut,
    book: Book,
    bolt: Lightning,
    home: House,
  };

  interface Props {
    name: string;
    size?: number | string;
    color?: string;
    weight?: 'thin' | 'light' | 'regular' | 'bold' | 'fill' | 'duotone';
    class?: string;
  }
  let { name, size = 16, color = 'currentColor', weight = 'regular', class: cls }: Props = $props();
  const Icon = $derived(ICONS[name] ?? FileText);
</script>

<Icon {size} {color} {weight} class={cls} aria-hidden="true" />
