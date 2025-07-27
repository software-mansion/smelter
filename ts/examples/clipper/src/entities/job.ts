export type JobStatus = 'pending' | 'done' | 'corrupted';

export type ClipJob = {
  id: number;
  status: JobStatus;
  /** Time at which clip was requested. */
  clipTimestamp: number;
  /** Target duration of the clip. */
  duration: number;
};
