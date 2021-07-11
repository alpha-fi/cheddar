# Token Farm with Fixed Supply

The P2-fixed farm allows to stake tokens and farm Cheddar. Constraints:
* The total supply of Cheddar is fixed = `total_cheddar`.
* Cheddar is farmed per round. During each round we farm `total_cheddar/number_rounds`.
* Each user, in each round will farm proportionally to the amount of staked tokens.

## Parameters
