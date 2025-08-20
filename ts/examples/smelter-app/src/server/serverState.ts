import { RoomState } from './roomState';
import { v4 as uuidv4 } from 'uuid';
import { SmelterManager } from '../smelter';
import { errorCodes } from 'fastify';

export type CreateRoomResult = {
  roomId: string;
  room: RoomState;
};

const ROOM_COUNT_SOFT_LIMIT = 3;
const ROOM_COUNT_HARD_LIMIT = 5;

class ServerState {
  private rooms: Record<string, RoomState> = {};
  private smelterManager: SmelterManager;

  constructor() {
    this.smelterManager = new SmelterManager();
    setInterval(async () => {
      await this.monitorConnectedRooms();
    }, 1000);
  }

  public async createRoom(): Promise<CreateRoomResult> {
    const roomId = uuidv4();
    const smelterOutput = await this.smelterManager.registerOutput(roomId);
    const room = new RoomState(roomId, smelterOutput);
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
    console.log(rooms);

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
    console.log(rooms);

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
          }, 20_000);
        } catch (err: any) {
          console.log(err, `Failed to remove room ${roomId}`);
        }
      }
    }
  }
}

export const state = new ServerState();
