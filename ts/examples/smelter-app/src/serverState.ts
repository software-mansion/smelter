import { RoomState } from './roomState';
import { v4 as uuidv4 } from 'uuid';
import { SmelterManager } from './smelter';

export type CreateRoomResult = {
  roomId: string;
  room: RoomState;
};

class ServerState {
  private rooms: Record<string, RoomState> = {};
  private smelterManager: SmelterManager;

  constructor() {
    this.smelterManager = new SmelterManager();
  }

  public async createRoom(): Promise<CreateRoomResult> {
    const roomId = uuidv4();
    const smelterOutput = await this.smelterManager.startNewOutput(roomId);
    const room = new RoomState(roomId, smelterOutput);
    return { roomId, room };
  }

  public getRoom(roomId: string): RoomState {
    const room = this.rooms[roomId];
    if (!room) {
      throw new Error(`Room ${roomId} does not exists.`);
    }
    return room;
  }

  public async deleteRoom(roomId: string) {
    const room = this.rooms[roomId];
    delete this.rooms[roomId];
    if (!room) {
      throw new Error(`Room ${roomId} does not exists.`);
    }
    // unregister everything
  }
}

export const state = new ServerState();
