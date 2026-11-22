export type Route =
  | { name: "home" }
  | { name: "manga"; id: number }
  | { name: "reader"; mangaId: number; chapterUrl: string; scrollRatio?: number }
  | { name: "history" }
  | { name: "settings" };

class RouterStore {
  current = $state<Route>({ name: "home" });

  goHome() {
    this.current = { name: "home" };
  }

  goManga(id: number) {
    this.current = { name: "manga", id };
  }

  goReader(mangaId: number, chapterUrl: string, scrollRatio?: number) {
    this.current = { name: "reader", mangaId, chapterUrl, scrollRatio };
  }

  goHistory() {
    this.current = { name: "history" };
  }

  goSettings() {
    this.current = { name: "settings" };
  }
}

export const router = new RouterStore();
