set -e
declare -a my_accs
while IFS=, read -r near_id; 
do
  # do something... Don't forget to skip the header line!
  my_accs[0]=$near_id
  #my_array(near_id)
done < csv_accounts/council.csv;

echo ${my_accs[@]","}