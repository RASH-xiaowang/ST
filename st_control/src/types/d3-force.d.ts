declare module "d3-force" {
  export interface Simulation<NodeDatum, LinkDatum> {
    nodes(): NodeDatum[];
    nodes(nodes: NodeDatum[]): this;
    alpha(): number;
    alpha(alpha: number): this;
    alphaTarget(): number;
    alphaTarget(alpha: number): this;
    alphaDecay(decay: number): this;
    velocityDecay(decay: number): this;
    restart(): this;
    stop(): this;
    on(typenames: string, listener: () => void): this;
    force(name: string): Force<NodeDatum, LinkDatum> | undefined;
    force(name: string, force: Force<NodeDatum, LinkDatum>): this;
  }

  export interface Force<NodeDatum, LinkDatum> {
    (alpha: number): void;
    initialize?(nodes: NodeDatum[], random?: () => number): void;
  }

  export interface ForceLink<NodeDatum, LinkDatum> extends Force<NodeDatum, LinkDatum> {
    links(): LinkDatum[];
    links(links: LinkDatum[]): this;
    id(accessor: (d: NodeDatum, i: number, data: NodeDatum[]) => string | number): this;
    distance(accessor: number | ((d: LinkDatum, i: number) => number)): this;
    strength(accessor: number | ((d: LinkDatum, i: number) => number)): this;
  }

  export interface ForceManyBody<NodeDatum> extends Force<NodeDatum, unknown> {
    strength(strength: number | ((d: NodeDatum, i: number, data: NodeDatum[]) => number)): this;
    theta(theta: number): this;
    distanceMin(distance: number): this;
    distanceMax(distance: number): this;
  }

  export interface ForceCenter<NodeDatum> extends Force<NodeDatum, unknown> {
    x(x: number): this;
    y(y: number): this;
    strength(strength: number): this;
  }

  export interface ForceCollide<NodeDatum> extends Force<NodeDatum, unknown> {
    radius(accessor: number | ((d: NodeDatum, i: number, data: NodeDatum[]) => number)): this;
    strength(strength: number): this;
    iterations(iterations: number): this;
  }

  export function forceSimulation<NodeDatum, LinkDatum = unknown>(
    nodes?: NodeDatum[],
  ): Simulation<NodeDatum, LinkDatum>;
  export function forceLink<NodeDatum, LinkDatum>(links?: LinkDatum[]): ForceLink<NodeDatum, LinkDatum>;
  export function forceManyBody<NodeDatum>(): ForceManyBody<NodeDatum>;
  export function forceCollide<NodeDatum>(): ForceCollide<NodeDatum>;
  export function forceCenter<NodeDatum>(x?: number, y?: number): ForceCenter<NodeDatum>;
}
