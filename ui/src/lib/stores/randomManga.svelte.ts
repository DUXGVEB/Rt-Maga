export type RandomMangaScope = "all" | "bookmarks";

class RandomMangaStore {
  isOpen = $state(false);
  scope = $state<RandomMangaScope>("all");
  lastPickedId = $state<number | null>(null);

  open(initialScope: RandomMangaScope = "all") {
    this.scope = initialScope;
    this.isOpen = true;
  }

  close() {
    this.isOpen = false;
  }

  rememberPick(id: number) {
    this.lastPickedId = id;
  }
}

export const randomMangaStore = new RandomMangaStore();
