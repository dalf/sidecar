Hack and dirty code to create instant search from Wikidata.

Note for later:
* https://aidanhogan.com/docs/infobox-wikidata.pdf


* Q4200 : /info/en/4200 bug
* Q2222461 : impossible to find it by query ( "/query/false/Rothschild giraffe" doesn't work)


----

Note:
* keep wd:Q13406463 (Wikimedia list article)

TODO:
* build an index per language (most problably it will explore the disk size).
* What to do about wd:Q4167836 (Wikimedia category) and wd:Q4167410 (Wikimedia disambiguation page) 
    --> store content but do not index them
* ?item wdt:P31 wd:Q13442814 and Q7318358 without wikipedia links
    --> index in a "science" index
* ?item wdt:P31 wd:Q523 (star) / wd:Q318 (galaxy) / wd:Q11173 (chemical compound) / Q75140589 (eclipsing binary star)
    --> index in a "raw" index (not tokenization without simplification)
* 


TODO with infinte time:
* parse the Wikimedia disambiguation page and record suggestion.

TO TEST:
* Ignore ?item wdt:P31 wd:Q11266439 # Wikimedia template
