use rand::Rng;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::{self, Duration};
// A static list of 100 jokes
const JOKES: &[&str] = &[
    "Why don't scientists trust atoms? Because they make up everything!",
    "Why did the scarecrow win an award? Because he was outstanding in his field!",
    "Why don't skeletons fight each other? They don't have the guts.",
    "What do you call fake spaghetti? An impasta!",
    "Why did the bicycle fall over? Because it was two-tired!",
    "Why can't your nose be 12 inches long? Because then it would be a foot!",
    "What do you call cheese that isn't yours? Nacho cheese!",
    "Why did the golfer bring two pairs of pants? In case he got a hole in one!",
    "Why did the math book look sad? Because it had too many problems.",
    "Why can't you give Elsa a balloon? Because she will let it go!",
    "Why did the tomato turn red? Because it saw the salad dressing!",
    "Why did the computer go to the doctor? Because it had a virus!",
    "Why did the chicken join a band? Because it had the drumsticks!",
    "Why did the coffee file a police report? It got mugged!",
    "Why did the cookie go to the doctor? Because it was feeling crumby!",
    "Why did the man put his money in the blender? He wanted liquid assets!",
    "Why did the picture go to jail? Because it was framed!",
    "Why did the banana go to the doctor? Because it wasn't peeling well!",
    "Why did the cow go to outer space? To see the Milky Way!",
    "Why did the fish blush? Because it saw the ocean's bottom!",
    "Why did the music teacher go to jail? Because she got caught with the wrong notes!",
    "Why did the frog take the bus to work? His car got toad away!",
    "Why did the belt get arrested? For holding up a pair of pants!",
    "Why did the calendar go to therapy? It had too many dates!",
    "Why did the barber win the race? He knew all the shortcuts!",
    "Why did the grape stop in the middle of the road? Because it ran out of juice!",
    "Why did the clock get kicked out of class? It was tocking too much!",
    "Why did the skeleton go to the party alone? He had no body to go with him!",
    "Why did the dog sit in the shade? Because he didn't want to be a hot dog!",
    "Why did the cat sit on the computer? To keep an eye on the mouse!",
    "Why did the duck go to the doctor? Because it was feeling a little quacky!",
    "Why did the elephant bring a suitcase? Because it was going on a trunk trip!",
    "Why did the bee get married? Because he found his honey!",
    "Why did the tree go to the dentist? To get a root canal!",
    "Why did the book join the police? Because it wanted to go undercover!",
    "Why did the pencil go to art school? Because it wanted to draw attention!",
    "Why did the light bulb go to school? It wanted to be brighter!",
    "Why did the pirate go to the gym? To improve his arrrrms!",
    "Why did the astronaut break up with his girlfriend? He needed space!",
    "Why did the baker go to therapy? He kneaded it!",
    "Why did the computer get cold? It left its Windows open!",
    "Why did the vampire go to the doctor? He was feeling a little drained!",
    "Why did the robot go to school? To improve its programming!",
    "Why did the bird go to the library? To find tweet books!",
    "Why did the fish go to the library? To find some good fish tales!",
    "Why did the squirrel go to the gym? To work on his nuts!",
    "Why did the penguin go to the party? Because he was cool!",
    "Why did the owl go to the library? To improve his hoot knowledge!",
    "Why did the kangaroo go to the bar? To get a hop drink!",
    "Why did the snake go to the doctor? It had a hissy fit!",
    "Why did the turtle go to the party? To shell-ebrate!",
    "Why did the rabbit go to the library? To find some hare-raising tales!",
    "Why did the horse go to the bar? To neigh and relax!",
    "Why did the lion go to the library? To improve his roar knowledge!",
    "Why did the monkey go to the party? To go bananas!",
    "Why did the giraffe go to the library? To reach the top shelf!",
    "Why did the elephant go to the library? To find some trunk tales!",
    "Why did the zebra go to the library? To find some striped tales!",
    "Why did the bear go to the library? To find some grizzly tales!",
    "Why did the fox go to the library? To find some sly tales!",
    "Why did the wolf go to the library? To find some howling tales!",
    "Why did the deer go to the library? To find some fawn tales!",
    "Why did the moose go to the library? To find some antler tales!",
    "Why did the raccoon go to the library? To find some trashy tales!",
    "Why did the skunk go to the library? To find some stinky tales!",
    "Why did the porcupine go to the library? To find some prickly tales!",
    "Why did the hedgehog go to the library? To find some spiky tales!",
    "Why did the armadillo go to the library? To find some armored tales!",
    "Why did the platypus go to the library? To find some duck-billed tales!",
    "Why did the sloth go to the library? To find some slow tales!",
    "Why did the koala go to the library? To find some eucalyptus tales!",
    "Why did the panda go to the library? To find some bamboo tales!",
    "Why did the penguin go to the library? To find some icy tales!",
    "Why did the polar bear go to the library? To find some Arctic tales!",
    "Why did the walrus go to the library? To find some tusk tales!",
    "Why did the seal go to the library? To find some flipper tales!",
    "Why did the dolphin go to the library? To find some fin tales!",
    "Why did the shark go to the library? To find some toothy tales!",
    "Why did the whale go to the library? To find some blubbery tales!",
    "Why did the octopus go to the library? To find some tentacle tales!",
    "Why did the crab go to the library? To find some claw tales!",
    "Why did the lobster go to the library? To find some shell tales!",
    "Why did the shrimp go to the library? To find some small tales!",
    "Why did the clam go to the library? To find some pearl tales!",
    "Why did the oyster go to the library? To find some shellfish tales!",
    "Why did the starfish go to the library? To find some stellar tales!",
    "Why did the jellyfish go to the library? To find some wobbly tales!",
    "Why did the seahorse go to the library? To find some aquatic tales!",
    "Why did the coral go to the library? To find some reef tales!",
    "Why did the seaweed go to the library? To find some leafy tales!",
    "Why did the kelp go to the library? To find some underwater tales!",
    "Why did the barnacle go to the library? To find some sticky tales!",
    "Why did the anemone go to the library? To find some flowery tales!",
];

/// Returns the next joke in the list, cycling back to the beginning after the last joke.
pub fn get_next_joke() -> &'static str {
    let mut rng = rand::rng();
    let index = rng.random_range(0..JOKES.len());
    JOKE_INDEX.store(index, Ordering::Relaxed);
    JOKES[index]
}

// Atomic counter to keep track of the current joke index
static JOKE_INDEX: AtomicUsize = AtomicUsize::new(0);

/// Returns the current joke without advancing the index.
pub fn get_curr_joke() -> &'static str {
    let index = JOKE_INDEX.load(Ordering::Relaxed) % JOKES.len();
    JOKES[index]
}

/// Advances the joke index and returns the next joke.
#[allow(dead_code)]
pub fn get_seq_joke() -> &'static str {
    let index = JOKE_INDEX.fetch_add(1, Ordering::Relaxed) % JOKES.len();
    JOKES[index]
}

/// Starts a background task to update the joke every 5 minutes.
pub fn start_joke_updater() {
    tokio::spawn(async {
        let mut interval = time::interval(Duration::from_secs(120));
        loop {
            interval.tick().await;
            get_next_joke(); // Update the joke
        }
    });
}
