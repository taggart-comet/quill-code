class Cat:
    def __init__(self, name: str, age: int):
        self.name = name
        self.age = age
        self.mood = "neutral"

    def meow(self) -> str:
        return f"{self.name} says: Meow!"

    def feed(self) -> str:
        self.mood = "happy"
        return f"{self.name} is eating. Nom nom nom!"

    def pet(self) -> str:
        if self.mood == "happy":
            return f"{self.name} purrs loudly."
        return f"{self.name} tolerates the petting."

    def describe(self) -> str:
        return f"{self.name} is a {self.age} year old cat feeling {self.mood}."

    def shit(self) -> str:
        self.mood = "relieved"
        return f"{self.name} uses the litter box and looks relieved."

def create_cat(name: str, age: int) -> Cat:
    return Cat(name, age)