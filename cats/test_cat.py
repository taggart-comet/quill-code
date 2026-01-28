from cat import Cat, create_cat


def test_cat_creation():
    cat = Cat("Whiskers", 3)
    assert cat.name == "Whiskers"
    assert cat.age == 3
    assert cat.mood == "neutral"


def test_meow():
    cat = Cat("Luna", 2)
    assert cat.meow() == "Luna says: Meow!"


def test_feed_changes_mood():
    cat = Cat("Oliver", 5)
    assert cat.mood == "neutral"
    cat.feed()
    assert cat.mood == "happy"


def test_pet_when_neutral():
    cat = Cat("Milo", 1)
    result = cat.pet()
    assert "tolerates" in result


def test_pet_when_happy():
    cat = Cat("Bella", 4)
    cat.feed()
    result = cat.pet()
    assert "purrs" in result


def test_describe():
    cat = Cat("Max", 7)
    desc = cat.describe()
    assert "Max" in desc
    assert "7" in desc
    assert "neutral" in desc


def test_begs_changes_mood():
    cat = Cat("Nala", 3)
    result = cat.begs()
    assert cat.mood == "needy"
    assert "begs" in result
    assert "snacks" in result


def test_create_cat_helper():
    cat = create_cat("Simba", 2)
    assert isinstance(cat, Cat)
    assert cat.name == "Simba"


def run_all_tests() -> None:
    tests = [
        test_cat_creation,
        test_meow,
        test_feed_changes_mood,
        test_pet_when_neutral,
        test_pet_when_happy,
        test_describe,
        test_begs_changes_mood,
        test_create_cat_helper,
    ]

    for test in tests:
        test()


if __name__ == "__main__":
    run_all_tests()
    print("All tests passed!")