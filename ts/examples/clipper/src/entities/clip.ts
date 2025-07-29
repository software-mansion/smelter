export type ClipStatus = 'pending' | 'done' | 'corrupted';

export type Clip = {
  id: number;
  name: string;
  status: ClipStatus;
  /** Time at which clip was requested. */
  clipTimestamp: number;
  /** Target duration of the clip. */
  duration: number;
  /** Outpu clip filename. */
  filename: string | null;
  createdAt: string;
  updatedAt: string;
};
