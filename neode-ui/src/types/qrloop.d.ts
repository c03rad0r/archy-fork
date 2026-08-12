declare module 'qrloop' {
  export type FramesState = unknown
  export function parseFramesReducer(state: FramesState | null, frame: string): FramesState
  export function areFramesComplete(state: FramesState): boolean
  export function framesToData(state: FramesState): { toString(encoding: string): string }
  export function totalNumberOfFrames(state: FramesState): number
  export function currentNumberOfFrames(state: FramesState): number
  export function progressOfFrames(state: FramesState): number
}
