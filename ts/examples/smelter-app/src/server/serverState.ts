import type { RegisterInputOptions } from './roomState';
import { RoomState } from './roomState';
import { v4 as uuidv4 } from 'uuid';
import { errorCodes } from 'fastify';
import { SmelterInstance } from '../smelter';

export type CreateRoomResult = {
  roomId: string;
  room: RoomState;
};

const ROOM_COUNT_SOFT_LIMIT = 3;
const ROOM_COUNT_HARD_LIMIT = 5;
const SOFT_LIMIT_ROOM_DELETE_DELAY = 20_000;
const WHIP_STALE_TTL_MS = 15_000;

class ServerState {
  private rooms: Record<string, RoomState> = {};
  public getRooms(): RoomState[] {
    return Object.values(this.rooms);
  }

  public isChannelIdUsed(channelId: string): boolean {
    return this.getRooms().some(room =>
      room
        .getInputs()
        .some(
          input =>
            (input.type === 'kick-channel' || input.type === 'twitch-channel') &&
            input.channelId === channelId
        )
    );
  }

  constructor() {
    setInterval(async () => {
      await this.monitorConnectedRooms();
    }, 1000);
  }

  public async createRoom(initInputs: RegisterInputOptions[]): Promise<CreateRoomResult> {
    const roomId = uuidv4();
    const smelterOutput = await SmelterInstance.registerOutput(roomId);
    const room = new RoomState(roomId, smelterOutput, initInputs);
    this.rooms[roomId] = room;
    return { roomId, room };
  }

  public getRoom(roomId: string): RoomState {
    const room = this.rooms[roomId];
    if (!room) {
      throw new errorCodes.FST_ERR_NOT_FOUND(`Room ${roomId} does not exists.`);
    }
    return room;
  }

  public async deleteRoom(roomId: string) {
    const room = this.rooms[roomId];
    delete this.rooms[roomId];
    if (!room) {
      throw new Error(`Room ${roomId} does not exists.`);
    }
    await room.deleteRoom();
  }

  private async monitorConnectedRooms() {
    let rooms = Object.entries(this.rooms);
    rooms.sort(([_aId, aRoom], [_bId, bRoom]) => bRoom.creationTimestamp - aRoom.creationTimestamp);
    // Remove WHIP inputs that haven't acked within 15 s
    for (const [_roomId, room] of rooms) {
      await room.removeStaleWhipInputs(WHIP_STALE_TTL_MS);
    }
    for (const [roomId, room] of rooms) {
      if (Date.now() - room.lastReadTimestamp > 60_000) {
        try {
          console.log('Stop from inactivity');
          await this.deleteRoom(roomId);
        } catch (err: any) {
          console.log(err, `Failed to remove room ${roomId}`);
        }
      }
    }

    // recalculate the rooms
    rooms = Object.entries(this.rooms);
    rooms.sort(([_aId, aRoom], [_bId, bRoom]) => bRoom.creationTimestamp - aRoom.creationTimestamp);

    if (rooms.length > ROOM_COUNT_HARD_LIMIT) {
      for (const [roomId, _room] of rooms.slice(ROOM_COUNT_HARD_LIMIT - rooms.length)) {
        try {
          console.log('Stop from hard limit');
          await this.deleteRoom(roomId).catch(() => {});
        } catch (err: any) {
          console.log(err, `Failed to remove room ${roomId}`);
        }
      }
    }

    if (rooms.length > ROOM_COUNT_SOFT_LIMIT) {
      for (const [roomId, room] of rooms.slice(ROOM_COUNT_SOFT_LIMIT - rooms.length)) {
        if (room.pendingDelete) {
          continue;
        }
        try {
          console.log('Schedule stop from soft limit');
          room.pendingDelete = true;
          setTimeout(async () => {
            console.log('Stop from soft limit');
            await this.deleteRoom(roomId).catch(() => {});
          }, SOFT_LIMIT_ROOM_DELETE_DELAY);
        } catch (err: any) {
          console.log(err, `Failed to remove room ${roomId}`);
        }
      }
    }
  }
}

export const state = new ServerState();
